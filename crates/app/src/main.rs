//! `f9-talk` binary entry point.
//!
//! M1 wires:
//! - clap CLI matching v0.3.x flags verbatim
//! - tracing-journald subscriber (`SYSLOG_IDENTIFIER=f9-talk`)
//! - secrets-file loader (env > `~/.config/F9_talk/secrets.env`)
//! - single-instance lock on the abstract Unix socket
//!   `\0f9-talk-instance-lock`
//! - uinput permission preflight
//! - hotkey listener + cpal mic streamer + headless session loop
//! - wake-from-suspend tokio task
//!
//! No STT, no UI, no actual typing yet — the goal is the M1 exit
//! criterion: hold F9 → see frames flow → release → log "would type:
//! [N frames]".

use std::collections::HashMap;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use f9_talk_input::{typer_preflight, HotkeyEvent, Typer};
use tokio::time::Instant;
use tracing::{debug, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const INSTANCE_LOCK_NAME: &[u8] = b"\0f9-talk-instance-lock";

#[derive(Parser, Debug)]
#[command(name = "f9-talk", version, about = "Hold-to-talk dictation for Linux")]
struct Cli {
    /// STT backend selection.
    #[arg(long, value_enum, default_value_t = Backend::Cloud)]
    backend: Backend,

    /// Hotkey for the local Whisper backend (held while speaking).
    #[arg(long, default_value = "f9")]
    local_hotkey: String,

    /// Hotkey for the cloud backend (held while speaking).
    #[arg(long, default_value = "f8")]
    cloud_hotkey: String,

    /// Translate transcripts to this ISO language code before typing.
    #[arg(long)]
    target: Option<String>,

    /// Domain-specific terms to bias STT toward (proper nouns, jargon). Repeatable.
    #[arg(long = "keyword")]
    keywords: Vec<String>,

    /// Indicator animation style. v1.0 only ships `wave`; others fall back to wave.
    #[arg(long, default_value = "wave")]
    style: String,

    /// Verbose (debug-level) logging.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Backend {
    Cloud,
    Local,
    Both,
}

fn main() -> anyhow::Result<()> {
    init_tracing(parse_verbose());

    let cli = Cli::parse();
    if cli.verbose {
        debug!("CLI: {cli:?}");
    }

    // Single-instance lock — must be the FIRST thing we do so a second
    // copy never loads CUDA / opens the mic / etc.
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

    if matches!(cli.style.as_str(), "wave") {
        debug!("indicator style: wave");
    } else {
        warn!(
            "indicator style {:?} not in v1; falling back to 'wave'",
            cli.style
        );
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("f9-talk")
        .build()?;

    let active_chord = match cli.backend {
        // In cloud-only mode the local-hotkey arg becomes the cloud hotkey,
        // matching `app.py:__init__`.
        Backend::Cloud => cli.local_hotkey.clone(),
        Backend::Local => cli.local_hotkey.clone(),
        Backend::Both => cli.local_hotkey.clone(),
    };

    runtime.block_on(async move { run_session_loop(&active_chord).await })
}

fn parse_verbose() -> bool {
    std::env::args().any(|a| a == "-v" || a == "--verbose")
}

fn init_tracing(verbose: bool) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(if verbose { "debug" } else { "info" })
    });

    let registry = tracing_subscriber::registry().with(env_filter);

    // tracing-journald sets the SYSLOG_IDENTIFIER from the binary name,
    // which is `f9-talk` — matching the existing `journalctl --user -t f9-talk`.
    match tracing_journald::layer() {
        Ok(journald) => {
            registry
                .with(journald)
                .with(tracing_subscriber::fmt::layer().with_target(false))
                .init();
        }
        Err(e) => {
            // Falls back to stderr-only logging when journald is unavailable
            // (e.g. inside a container). Rare but not a hard failure.
            eprintln!("journald layer unavailable ({e}); logging to stderr only");
            registry
                .with(tracing_subscriber::fmt::layer().with_target(false))
                .init();
        }
    }
}

fn acquire_instance_lock() -> anyhow::Result<UnixDatagram> {
    let socket = UnixDatagram::unbound()?;
    // Linux abstract namespace: leading null byte means the socket is
    // not on the filesystem; std doesn't expose it directly so we
    // reach through to libc::bind.
    bind_abstract(&socket, INSTANCE_LOCK_NAME)?;
    Ok(socket)
}

fn bind_abstract(sock: &UnixDatagram, name: &[u8]) -> anyhow::Result<()> {
    // SAFETY: we construct a sockaddr_un with a leading NUL and pass
    // its precise byte length to bind(). This is the documented Linux
    // abstract-namespace API.
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

    // 1. env vars take priority
    for key in ["DEEPGRAM_API_KEY", "ASSEMBLYAI_API_KEY", "GLADIA_API_KEY"] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                out.insert(key.to_string(), v);
            }
        }
    }

    // 2. ~/.config/F9_talk/secrets.env
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

async fn run_session_loop(chord: &str) -> anyhow::Result<()> {
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

    let (mut frame_rx, _rms, _mic_task) =
        f9_talk_audio::spawn().map_err(|e| anyhow::anyhow!("could not start mic streamer: {e}"))?;

    info!(
        "f9-talk M1 ready. hold {} to record (Ctrl-C to quit)",
        chord
    );
    let _typer = Typer::new()?;

    spawn_wakeup_watcher();

    let mut session_frames: u64 = 0;
    let mut recording = false;

    loop {
        tokio::select! {
            evt = hotkey_rx.recv() => {
                match evt {
                    Some(HotkeyEvent::Pressed) => {
                        recording = true;
                        session_frames = 0;
                        info!("🎙  recording…");
                    }
                    Some(HotkeyEvent::Released) => {
                        recording = false;
                        info!("✏  release: would type [N frames] = {session_frames}");
                    }
                    None => {
                        warn!("hotkey channel closed; exiting");
                        return Ok(());
                    }
                }
            }
            frame = frame_rx.recv() => {
                let Some(_f) = frame else {
                    warn!("mic channel closed; exiting");
                    return Ok(());
                };
                if recording {
                    session_frames += 1;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received; shutting down");
                return Ok(());
            }
        }
    }
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
                // M2 will broadcast this to STT clients via a Notify;
                // for M1 we just log so the test "leave running, suspend,
                // resume" produces visible evidence.
            }
            last = now;
        }
    });
}

// Required for the libc::bind call above.
extern crate libc;
