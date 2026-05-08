//! `f9-talk` binary entry point.
//!
//! Threading model (M3):
//! - **Main thread**: clap → instance lock → secrets → spawn tokio
//!   runtime → spawn mic + session loop on it → block in
//!   `eframe::run_native` driving the egui indicator.
//! - **Tokio runtime worker thread(s)**: STT WS clients, hotkey
//!   listener, mic frame router, session loop, wake-from-suspend.
//! - **cpal callback thread**: real-time, owned by cpal; pushes 25 ms
//!   frames + RMS into the shared `IndicatorState`.

use std::collections::HashMap;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use f9_talk_input::{typer_preflight, HotkeyEvent, Typer};
use f9_talk_stt::{BackendEvent, Stt};
use f9_talk_ui::{IndicatorApp, IndicatorState};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const INSTANCE_LOCK_NAME: &[u8] = b"\0f9-talk-instance-lock";

#[derive(Parser, Debug, Clone)]
#[command(name = "f9-talk", version, about = "Hold-to-talk dictation for Linux")]
struct Cli {
    #[arg(long, value_enum, default_value_t = Backend::Cloud)]
    backend: Backend,

    #[arg(long, default_value = "f9")]
    local_hotkey: String,

    #[arg(long, default_value = "f8")]
    cloud_hotkey: String,

    #[arg(long)]
    target: Option<String>,

    #[arg(long = "keyword")]
    keywords: Vec<String>,

    #[arg(long, default_value = "wave")]
    style: String,

    /// Force a specific cloud provider (overrides auto-select).
    #[arg(long, value_enum)]
    cloud_provider: Option<CloudProvider>,

    /// Run headless (no indicator window). Useful for the M2 smoke
    /// path or autostart on non-X11/Wayland sessions.
    #[arg(long)]
    headless: bool,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Backend {
    Cloud,
    Local,
    Both,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
enum CloudProvider {
    Assemblyai,
    Deepgram,
}

fn main() -> anyhow::Result<()> {
    init_tracing(parse_verbose());
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();
    if cli.verbose {
        debug!("CLI: {cli:?}");
    }

    let _lock = match acquire_instance_lock() {
        Ok(lock) => lock,
        Err(_) => {
            eprintln!("f9-talk is already running.");
            std::process::exit(0);
        }
    };

    if let Err(e) = typer_preflight() {
        eprintln!("\nf9-talk: {e}\n");
        std::process::exit(2);
    }

    let secrets = load_secrets();
    if cli.verbose {
        let masked: HashMap<&str, String> = secrets
            .iter()
            .map(|(k, v)| {
                let display = if v.is_empty() {
                    "<empty>".to_string()
                } else {
                    format!("<set, {} chars>", v.len())
                };
                (k.as_str(), display)
            })
            .collect();
        debug!("loaded secrets: {masked:?}");
    }

    if !matches!(cli.style.as_str(), "wave") {
        warn!(
            "indicator style {:?} not in v1; falling back to 'wave'",
            cli.style
        );
    }

    let provider = pick_cloud_provider(cli.cloud_provider, &secrets);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("f9-talk")
        .build()?;

    // Mic streamer must spawn inside a runtime context.
    let _guard = runtime.enter();
    let (frame_rx, rms_handle, _mic_task) =
        f9_talk_audio::spawn().map_err(|e| anyhow::anyhow!("could not start mic streamer: {e}"))?;
    drop(_guard);

    let indicator_state = Arc::new(IndicatorState::new(rms_handle));

    let chord = cli.local_hotkey.clone();
    let cli_for_task = cli.clone();
    let secrets_for_task = secrets.clone();
    let state_for_task = indicator_state.clone();
    runtime.spawn(async move {
        if let Err(e) = run_session_loop(
            &chord,
            provider,
            &cli_for_task,
            &secrets_for_task,
            frame_rx,
            state_for_task,
        )
        .await
        {
            tracing::error!("session loop error: {e}");
        }
    });

    if cli.headless {
        info!("--headless: blocking on Ctrl-C (no indicator window)");
        runtime.block_on(async {
            tokio::signal::ctrl_c().await.ok();
        });
        return Ok(());
    }

    // Indicator window dimensions: 360 × 80 — wider than the 320 px
    // pill so the wave layers' soft glow doesn't clip at the edges.
    let viewport = egui::ViewportBuilder::default()
        .with_title("f9-talk")
        .with_app_id("f9-talk")
        .with_inner_size([360.0, 80.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_resizable(false)
        .with_taskbar(false)
        .with_mouse_passthrough(true);
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let state_for_app = indicator_state.clone();
    eframe::run_native(
        "f9-talk",
        native_options,
        Box::new(move |_cc| Ok(Box::new(IndicatorApp::new(state_for_app)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    info!("indicator closed; shutting down");
    runtime.shutdown_timeout(Duration::from_secs(2));
    Ok(())
}

fn parse_verbose() -> bool {
    std::env::args().any(|a| a == "-v" || a == "--verbose")
}

fn init_tracing(verbose: bool) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(if verbose { "debug" } else { "info" })
    });
    let registry = tracing_subscriber::registry().with(env_filter);
    match tracing_journald::layer() {
        Ok(journald) => {
            registry
                .with(journald)
                .with(tracing_subscriber::fmt::layer().with_target(false))
                .init();
        }
        Err(e) => {
            eprintln!("journald layer unavailable ({e}); logging to stderr only");
            registry
                .with(tracing_subscriber::fmt::layer().with_target(false))
                .init();
        }
    }
}

fn acquire_instance_lock() -> anyhow::Result<UnixDatagram> {
    let socket = UnixDatagram::unbound()?;
    bind_abstract(&socket, INSTANCE_LOCK_NAME)?;
    Ok(socket)
}

fn bind_abstract(sock: &UnixDatagram, name: &[u8]) -> anyhow::Result<()> {
    use std::os::fd::AsRawFd;
    if name.len() > 107 {
        anyhow::bail!("abstract socket name too long: {} bytes", name.len());
    }
    let fd = sock.as_raw_fd();
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (i, b) in name.iter().enumerate() {
        addr.sun_path[i] = *b as libc::c_char;
    }
    let addrlen = (std::mem::size_of::<libc::sa_family_t>() + name.len()) as libc::socklen_t;
    let rc = unsafe { libc::bind(fd, &addr as *const _ as *const libc::sockaddr, addrlen) };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        anyhow::bail!("bind on abstract socket failed: {err}");
    }
    Ok(())
}

fn load_secrets() -> HashMap<String, String> {
    let mut out = HashMap::new();
    for key in ["DEEPGRAM_API_KEY", "ASSEMBLYAI_API_KEY", "GLADIA_API_KEY"] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                out.insert(key.to_string(), v);
            }
        }
    }
    if let Some(path) = secrets_path() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim().to_string();
                    let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
                    out.entry(k).or_insert(v);
                }
            }
        }
    }
    out
}

fn secrets_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/F9_talk/secrets.env"))
}

fn pick_cloud_provider(
    forced: Option<CloudProvider>,
    secrets: &HashMap<String, String>,
) -> Option<CloudProvider> {
    if let Some(p) = forced {
        return Some(p);
    }
    let has_aai = secrets.contains_key("ASSEMBLYAI_API_KEY");
    let has_dg = secrets.contains_key("DEEPGRAM_API_KEY");
    if has_aai {
        Some(CloudProvider::Assemblyai)
    } else if has_dg {
        Some(CloudProvider::Deepgram)
    } else {
        None
    }
}

async fn build_cloud_backend(
    provider: CloudProvider,
    secrets: &HashMap<String, String>,
    keywords: &[String],
) -> anyhow::Result<Arc<dyn Stt>> {
    match provider {
        CloudProvider::Assemblyai => {
            let key = secrets
                .get("ASSEMBLYAI_API_KEY")
                .cloned()
                .unwrap_or_default();
            Ok(Arc::new(f9_talk_stt::assemblyai::AssemblyAi::new(
                key,
                f9_talk_stt::assemblyai::Config {
                    keyterms: keywords.to_vec(),
                },
            )))
        }
        CloudProvider::Deepgram => {
            let key = secrets.get("DEEPGRAM_API_KEY").cloned().unwrap_or_default();
            Ok(Arc::new(f9_talk_stt::deepgram::Deepgram::new(
                key,
                f9_talk_stt::deepgram::Config {
                    keywords: keywords.to_vec(),
                    ..Default::default()
                },
            )))
        }
    }
}

async fn run_session_loop(
    chord: &str,
    provider: Option<CloudProvider>,
    cli: &Cli,
    secrets: &HashMap<String, String>,
    mut frame_rx: mpsc::Receiver<f9_talk_audio::Frame>,
    indicator: Arc<IndicatorState>,
) -> anyhow::Result<()> {
    let provider = match (cli.backend, provider) {
        (Backend::Cloud, None) => {
            eprintln!(
                "f9-talk: --backend cloud needs ASSEMBLYAI_API_KEY or DEEPGRAM_API_KEY \
                 set in the environment or in ~/.config/F9_talk/secrets.env"
            );
            std::process::exit(2);
        }
        (Backend::Cloud, Some(p)) => p,
        (Backend::Local, _) | (Backend::Both, _) => {
            eprintln!(
                "f9-talk: --backend {{local|both}} support lands later in M2 \
                 (whisper-rs + CUDA model download)."
            );
            std::process::exit(2);
        }
    };

    let backend = build_cloud_backend(provider, secrets, &cli.keywords).await?;
    let backend_name = backend.name();

    let (event_tx, mut event_rx) = mpsc::channel::<BackendEvent>(64);
    backend
        .start(event_tx)
        .await
        .map_err(|e| anyhow::anyhow!("could not start STT backend ({backend_name}): {e}"))?;
    info!("STT backend ready: {backend_name}");

    let mut hotkey_rx = match f9_talk_input::spawn_hotkey(chord) {
        Ok(rx) => rx,
        Err(e) => {
            eprintln!(
                "f9-talk: could not start hotkey listener: {e}\n\
                 Are you a member of the `input` group? Run:\n\
                 \tsudo usermod -aG input $USER\n\
                 then log out and back in once."
            );
            std::process::exit(2);
        }
    };

    info!(
        "f9-talk M3 ready. hold {} to dictate (cloud={backend_name}, Ctrl-C to quit)",
        chord
    );
    let mut typer = Typer::new()?;

    spawn_wakeup_watcher();

    let mut session: Option<SessionInProgress> = None;

    loop {
        tokio::select! {
            evt = hotkey_rx.recv() => {
                match evt {
                    Some(HotkeyEvent::Pressed) => {
                        let press_at = Instant::now();
                        backend.begin_session().await;
                        indicator.set_recording(true);
                        indicator.set_status_text(None);
                        info!("🎙  recording…");
                        session = Some(SessionInProgress {
                            press_at,
                            first_byte_sent: None,
                            frames_sent: 0,
                        });
                    }
                    Some(HotkeyEvent::Released) => {
                        let Some(sess) = session.take() else { continue; };
                        let release_at = Instant::now();
                        indicator.set_recording(false);
                        indicator.set_status_text(Some("✏  Transcribing…".to_string()));
                        let result = backend.end_session(Duration::from_millis(350)).await;
                        let final_at = Instant::now();
                        info!(
                            target: "f9_talk::press",
                            "press_to_release={:.0?} frames={} first_byte_sent={:?} release_to_final={:.0?} transcript={:?}",
                            release_at.duration_since(sess.press_at),
                            sess.frames_sent,
                            sess.first_byte_sent.map(|t| t.duration_since(sess.press_at)),
                            final_at.duration_since(release_at),
                            result.transcript,
                        );
                        if result.transcript.is_empty() {
                            info!("(no speech detected)");
                            indicator.set_status_text(None);
                        } else {
                            indicator.set_status_text(Some("⌨  Typing…".to_string()));
                            if let Err(e) = typer.type_text(&result.transcript) {
                                warn!("typer failed: {e}");
                            }
                            indicator.set_status_text(None);
                        }
                    }
                    None => {
                        warn!("hotkey channel closed; exiting");
                        backend.stop().await;
                        return Ok(());
                    }
                }
            }
            frame = frame_rx.recv() => {
                let Some(f) = frame else {
                    warn!("mic channel closed; exiting");
                    backend.stop().await;
                    return Ok(());
                };
                if let Some(sess) = session.as_mut() {
                    if sess.first_byte_sent.is_none() {
                        sess.first_byte_sent = Some(Instant::now());
                    }
                    sess.frames_sent += 1;
                    backend.send_audio(&f.bytes).await;
                }
            }
            evt = event_rx.recv() => {
                match evt {
                    Some(BackendEvent::SocketLost(msg)) => warn!("STT socket lost: {msg}"),
                    Some(BackendEvent::SocketBack) => info!("STT socket reconnected"),
                    Some(BackendEvent::Error(e)) => warn!("STT error: {e}"),
                    None => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received; shutting down");
                backend.stop().await;
                return Ok(());
            }
        }
    }
}

struct SessionInProgress {
    press_at: Instant,
    first_byte_sent: Option<Instant>,
    frames_sent: u32,
}

fn spawn_wakeup_watcher() {
    tokio::spawn(async move {
        let mut last = Instant::now();
        let interval = Duration::from_secs(5);
        let threshold = Duration::from_secs(30);
        loop {
            tokio::time::sleep(interval).await;
            let now = Instant::now();
            let drift = now.duration_since(last);
            if drift > threshold {
                warn!(
                    "WakeUp event: clock advanced {:.0?} (>{:.0?} threshold). \
                    Long-lived connections should reconnect.",
                    drift, threshold
                );
            }
            last = now;
        }
    });
}

extern crate libc;
