//! Local STT via `whisper-rs` (whisper.cpp bindings).
//!
//! Defaults match the Python `f9_talk/stt/local_whisper.py`:
//! - model `ggml-large-v3-turbo.bin` (~1.5 GB)
//! - beam=5, VAD filter on, condition_on_previous_text = false
//! - keyword soft-prompt via `initial_prompt`
//!
//! Compile flavours:
//! - default: CPU only (no `nvcc` required)
//! - `--features cuda`: CUDA-accelerated (requires `nvidia-cuda-toolkit`)
//!
//! Lazy model download: triggered on the first F9 press in
//! `--backend local|both` if `~/.cache/f9-talk/models/ggml-large-v3-turbo.bin`
//! is absent.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::{BackendEvent, SessionResult, Stt, SttError};

const MODEL_FILE: &str = "ggml-large-v3-turbo.bin";
const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";

#[derive(Debug, Clone)]
pub struct Config {
    pub language: String,
    pub keywords: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "en".into(),
            keywords: vec![],
        }
    }
}

pub struct WhisperLocal {
    cfg: Config,
    state: Arc<SharedState>,
}

struct SharedState {
    /// Loaded whisper context, lazily on first end_session.
    ctx: Mutex<Option<WhisperCtxInner>>,
    /// Per-session PCM buffer (interleaved s16le @ 16 kHz mono).
    buffer: Mutex<Vec<u8>>,
    recording: Mutex<bool>,
    last_error: Mutex<Option<String>>,
    /// Optional channel back to the app for download progress / errors.
    events: Mutex<Option<mpsc::Sender<BackendEvent>>>,
}

#[cfg(feature = "whisper")]
struct WhisperCtxInner {
    ctx: whisper_rs::WhisperContext,
}

#[cfg(not(feature = "whisper"))]
struct WhisperCtxInner;

impl WhisperLocal {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            state: Arc::new(SharedState {
                ctx: Mutex::new(None),
                buffer: Mutex::new(Vec::with_capacity(16_000 * 2 * 4)), // ~4 s preallocated
                recording: Mutex::new(false),
                last_error: Mutex::new(None),
                events: Mutex::new(None),
            }),
        }
    }
}

pub fn model_cache_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".cache/f9-talk/models").join(MODEL_FILE)
}

#[async_trait]
impl Stt for WhisperLocal {
    fn name(&self) -> &'static str {
        "whisper-local"
    }

    async fn start(&self, events: mpsc::Sender<BackendEvent>) -> Result<(), SttError> {
        *self.state.events.lock() = Some(events);
        // We do NOT load the model here — that's deferred to the first
        // end_session call so cold start stays fast and so users who
        // run `--backend cloud` never pay the cost.
        info!(
            "whisper-local initialised (model load deferred until first F9 release; \
            CUDA={})",
            cfg!(feature = "cuda")
        );
        Ok(())
    }

    async fn begin_session(&self) {
        *self.state.recording.lock() = true;
        self.state.buffer.lock().clear();
        *self.state.last_error.lock() = None;
    }

    async fn send_audio(&self, pcm: &[u8]) {
        if !*self.state.recording.lock() {
            return;
        }
        self.state.buffer.lock().extend_from_slice(pcm);
    }

    async fn end_session(&self, _timeout: Duration) -> SessionResult {
        let started = Instant::now();
        *self.state.recording.lock() = false;

        // Move the audio out of the shared buffer so we can transcribe
        // without holding the lock.
        let pcm = std::mem::take(&mut *self.state.buffer.lock());
        if pcm.len() < 16_000 * 2 / 5 {
            // <0.2 s of audio; matches the Python "(too short, ignored)" rule.
            return SessionResult {
                transcript: String::new(),
                finalize_latency: started.elapsed(),
            };
        }

        let cfg = self.cfg.clone();
        let state = self.state.clone();

        let transcript = tokio::task::spawn_blocking(move || transcribe(state, cfg, pcm))
            .await
            .unwrap_or_else(|join_err| {
                warn!("whisper join error: {join_err}");
                String::new()
            });
        SessionResult {
            transcript,
            finalize_latency: started.elapsed(),
        }
    }

    async fn stop(&self) {
        // Drop the model (releases CUDA VRAM if loaded).
        *self.state.ctx.lock() = None;
        *self.state.events.lock() = None;
    }
}

#[cfg(not(feature = "whisper"))]
fn transcribe(state: Arc<SharedState>, _cfg: Config, _pcm: Vec<u8>) -> String {
    let _ = state;
    warn!(
        "whisper backend invoked but the binary was built without the `whisper` \
        feature; rebuild with `cargo build --features whisper`"
    );
    String::new()
}

#[cfg(feature = "whisper")]
fn transcribe(state: Arc<SharedState>, cfg: Config, pcm: Vec<u8>) -> String {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    // 1. Ensure model is on disk.
    let model_path = match ensure_model(&state) {
        Ok(p) => p,
        Err(e) => {
            warn!("could not obtain whisper model: {e}");
            return String::new();
        }
    };

    // 2. Lazy-load the context once, then reuse.
    {
        let mut slot = state.ctx.lock();
        if slot.is_none() {
            info!(
                "loading whisper-large-v3-turbo from {}…",
                model_path.display()
            );
            let t0 = Instant::now();
            let params = WhisperContextParameters::default();
            let ctx = match WhisperContext::new_with_params(
                model_path.to_string_lossy().as_ref(),
                params,
            ) {
                Ok(c) => c,
                Err(e) => {
                    warn!("WhisperContext::new failed: {e}");
                    return String::new();
                }
            };
            info!("whisper model loaded in {:.1?}", t0.elapsed());
            *slot = Some(WhisperCtxInner { ctx });
        }
    }

    // 3. int16 PCM → float32 [-1, 1]
    let samples_i16: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    let samples_f32: Vec<f32> = samples_i16
        .iter()
        .map(|s| *s as f32 / i16::MAX as f32)
        .collect();
    debug!("whisper transcribing {} samples", samples_f32.len());

    // 4. Run transcribe.
    let slot = state.ctx.lock();
    let inner = slot.as_ref().expect("ctx loaded above");
    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
    });
    params.set_language(Some(&cfg.language));
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_no_context(true);
    params.set_no_speech_thold(0.6);
    params.set_logprob_thold(-1.0);
    let prompt: Option<String> = if cfg.keywords.is_empty() {
        None
    } else {
        Some(cfg.keywords.join(", "))
    };
    if let Some(p) = prompt.as_deref() {
        params.set_initial_prompt(p);
    }

    let mut state_obj = match inner.ctx.create_state() {
        Ok(s) => s,
        Err(e) => {
            warn!("WhisperContext::create_state failed: {e}");
            return String::new();
        }
    };
    let t0 = Instant::now();
    if let Err(e) = state_obj.full(params, &samples_f32) {
        warn!("whisper full() failed: {e}");
        return String::new();
    }

    let mut text = String::new();
    let n_segments = state_obj.full_n_segments().unwrap_or(0);
    for i in 0..n_segments {
        if let Ok(s) = state_obj.full_get_segment_text(i) {
            text.push_str(&s);
        }
    }
    debug!(
        "whisper produced {} chars from {n_segments} segments in {:.1?}",
        text.len(),
        t0.elapsed()
    );
    text.trim().to_string()
}

#[cfg(feature = "whisper")]
fn ensure_model(state: &Arc<SharedState>) -> Result<PathBuf, anyhow::Error> {
    let path = model_cache_path();
    if path.exists() {
        return Ok(path);
    }

    info!(
        "whisper model not cached at {}; downloading {} (~1.5 GB) once…",
        path.display(),
        MODEL_URL
    );
    if let Some(events) = state.events.lock().clone() {
        let _ = events.try_send(BackendEvent::Error(format!(
            "Downloading whisper model from {MODEL_URL}…"
        )));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("downloading");

    // Use the same reqwest version that's already in the dep tree;
    // blocking client is fine here because we're in spawn_blocking.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60 * 30))
        .build()?;
    let mut resp = client.get(MODEL_URL).send()?.error_for_status()?;
    let mut out = std::fs::File::create(&tmp_path)?;
    let bytes = std::io::copy(&mut resp, &mut out)?;
    info!("downloaded {bytes} B → {}", tmp_path.display());
    std::fs::rename(&tmp_path, &path)?;
    Ok(path)
}
