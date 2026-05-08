//! AssemblyAI Universal-3 Pro Streaming over `tokio-tungstenite`.
//!
//! Wire protocol:
//! - URL: `wss://streaming.assemblyai.com/v3/ws?speech_model=u3-rt-pro&encoding=pcm_s16le&sample_rate=16000&token={KEY}`
//!   `keyterm=…` repeated up to 100 times for keyterms prompting.
//! - Send: raw int16 PCM bytes as binary frames (no JSON wrapper, no base64).
//! - Receive (text frames, JSON):
//!   - `{"type":"Begin","id":"…"}` — session ack
//!   - `{"type":"Turn","transcript":"…","end_of_turn":bool}` — accumulate
//!     non-empty transcripts; we ignore `end_of_turn` because F9 release
//!     is our session boundary.
//!   - `{"type":"Termination"}` — session closed
//! - Send at end-of-press: `{"type":"Terminate"}`
//! - Persistent WS, auto-reconnect on close + on three consecutive send
//!   failures (mirrors the Deepgram impl).
//!
//! Latency target: ≤150 ms P50, ≤240 ms P90 (per AssemblyAI March 2026 docs).

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{debug, info, trace, warn};
use url::Url;

use crate::{BackendEvent, SessionResult, Stt, SttError, STT_SAMPLE_RATE};

const RECONNECT_INITIAL: Duration = Duration::from_secs(1);
const RECONNECT_CAP: Duration = Duration::from_secs(30);
const MAX_KEYTERMS: usize = 100;
const MAX_KEYTERM_LEN: usize = 50;

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// AssemblyAI calls these "keyterms"; we feed them from --keyword.
    pub keyterms: Vec<String>,
}

pub struct AssemblyAi {
    api_key: String,
    cfg: Config,
    state: Arc<SharedState>,
    cmd_tx: Mutex<Option<mpsc::Sender<Cmd>>>,
}

struct SharedState {
    recording: Mutex<bool>,
    session_finals: Mutex<Vec<String>>,
    final_arrived: Notify,
    shutting_down: std::sync::atomic::AtomicBool,
}

enum Cmd {
    Audio(Vec<u8>),
    Terminate,
    Stop,
}

impl AssemblyAi {
    pub fn new(api_key: impl Into<String>, cfg: Config) -> Self {
        Self {
            api_key: api_key.into(),
            cfg,
            state: Arc::new(SharedState {
                recording: Mutex::new(false),
                session_finals: Mutex::new(Vec::new()),
                final_arrived: Notify::new(),
                shutting_down: std::sync::atomic::AtomicBool::new(false),
            }),
            cmd_tx: Mutex::new(None),
        }
    }

    fn build_url(&self) -> Result<Url, SttError> {
        let mut u = Url::parse("wss://streaming.assemblyai.com/v3/ws")
            .map_err(|e| SttError::Internal(e.to_string()))?;
        {
            let mut q = u.query_pairs_mut();
            q.append_pair("speech_model", "u3-rt-pro");
            q.append_pair("encoding", "pcm_s16le");
            q.append_pair("sample_rate", &STT_SAMPLE_RATE.to_string());
            q.append_pair("token", &self.api_key);
            for kw in self.cfg.keyterms.iter().take(MAX_KEYTERMS) {
                if kw.len() > MAX_KEYTERM_LEN {
                    warn!("keyterm too long ({} chars) — truncated", kw.len());
                    q.append_pair("keyterm", &kw[..MAX_KEYTERM_LEN]);
                } else {
                    q.append_pair("keyterm", kw);
                }
            }
        }
        Ok(u)
    }
}

#[async_trait]
impl Stt for AssemblyAi {
    fn name(&self) -> &'static str {
        "assemblyai"
    }

    async fn start(&self, events: mpsc::Sender<BackendEvent>) -> Result<(), SttError> {
        if self.api_key.is_empty() {
            return Err(SttError::MissingKey("ASSEMBLYAI_API_KEY"));
        }
        let url = self.build_url()?;
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>(256);
        *self.cmd_tx.lock() = Some(cmd_tx);

        let state = self.state.clone();
        state
            .shutting_down
            .store(false, std::sync::atomic::Ordering::Relaxed);

        tokio::spawn(reconnect_loop(url, cmd_rx, events, state));
        Ok(())
    }

    async fn begin_session(&self) {
        *self.state.recording.lock() = true;
        self.state.session_finals.lock().clear();
    }

    async fn send_audio(&self, pcm: &[u8]) {
        if !*self.state.recording.lock() {
            return;
        }
        let Some(tx) = self.cmd_tx.lock().clone() else {
            return;
        };
        let _ = tx.try_send(Cmd::Audio(pcm.to_vec()));
    }

    async fn end_session(&self, timeout: Duration) -> SessionResult {
        let started = Instant::now();
        *self.state.recording.lock() = false;

        if let Some(tx) = self.cmd_tx.lock().clone() {
            let _ = tx.try_send(Cmd::Terminate);
        }

        let _ = tokio::time::timeout(timeout, self.state.final_arrived.notified()).await;
        tokio::time::sleep(Duration::from_millis(30)).await;

        let transcript = self
            .state
            .session_finals
            .lock()
            .join(" ")
            .trim()
            .to_string();
        SessionResult {
            transcript,
            finalize_latency: started.elapsed(),
        }
    }

    async fn stop(&self) {
        self.state
            .shutting_down
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(tx) = self.cmd_tx.lock().clone() {
            let _ = tx.try_send(Cmd::Stop);
        }
    }
}

async fn reconnect_loop(
    url: Url,
    mut cmd_rx: mpsc::Receiver<Cmd>,
    events: mpsc::Sender<BackendEvent>,
    state: Arc<SharedState>,
) {
    let mut backoff = RECONNECT_INITIAL;
    let mut have_been_connected_once = false;
    loop {
        if state
            .shutting_down
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        match run_connection(&url, &mut cmd_rx, &state).await {
            ConnectionEnd::Stop => return,
            ConnectionEnd::Closed => {
                if state
                    .shutting_down
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    return;
                }
                let _ = events
                    .send(BackendEvent::SocketLost(
                        "AssemblyAI socket closed; reconnecting".into(),
                    ))
                    .await;
            }
            ConnectionEnd::Error(e) => {
                let _ = events.send(BackendEvent::Error(e)).await;
            }
        }

        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(RECONNECT_CAP);
        if state
            .shutting_down
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }

        if have_been_connected_once {
            let _ = events.send(BackendEvent::SocketBack).await;
        }
        have_been_connected_once = true;
    }
}

#[derive(Debug)]
enum ConnectionEnd {
    Stop,
    Closed,
    Error(String),
}

async fn run_connection(
    url: &Url,
    cmd_rx: &mut mpsc::Receiver<Cmd>,
    state: &Arc<SharedState>,
) -> ConnectionEnd {
    let req = match url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => return ConnectionEnd::Error(format!("bad request: {e}")),
    };

    let stream = match connect_async(req).await {
        Ok((s, _resp)) => s,
        Err(e) => return ConnectionEnd::Error(format!("connect: {e}")),
    };
    info!("AssemblyAI socket open");

    let outcome = pump_messages(stream, cmd_rx, state).await;
    info!("AssemblyAI socket closed: {outcome:?}");
    outcome
}

async fn pump_messages(
    stream: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    cmd_rx: &mut mpsc::Receiver<Cmd>,
    state: &Arc<SharedState>,
) -> ConnectionEnd {
    let (mut sink, mut source) = stream.split();
    loop {
        tokio::select! {
            biased;
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(Cmd::Audio(bytes)) => {
                        if let Err(e) = sink.send(Message::Binary(bytes.into())).await {
                            return ConnectionEnd::Error(format!("audio send: {e}"));
                        }
                    }
                    Some(Cmd::Terminate) => {
                        if let Err(e) = sink.send(Message::Text(r#"{"type":"Terminate"}"#.into())).await {
                            warn!("Terminate send failed: {e}");
                        }
                    }
                    Some(Cmd::Stop) => {
                        let _ = sink.send(Message::Close(None)).await;
                        return ConnectionEnd::Stop;
                    }
                    None => return ConnectionEnd::Stop,
                }
            }
            msg = source.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) => handle_text(&t, state),
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Ping(p))) => { let _ = sink.send(Message::Pong(p)).await; }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Ok(Message::Frame(_))) | None => {
                        return ConnectionEnd::Closed;
                    }
                    Some(Err(e)) => return ConnectionEnd::Error(format!("ws read: {e}")),
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct AaiMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    transcript: Option<String>,
    #[allow(dead_code)]
    end_of_turn: Option<bool>,
    id: Option<String>,
}

fn handle_text(text: &str, state: &Arc<SharedState>) {
    let parsed: AaiMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            trace!("aai: non-JSON / unknown payload ({e}): {text:?}");
            return;
        }
    };
    match parsed.msg_type.as_deref() {
        Some("Begin") => {
            debug!("aai: session begin id={:?}", parsed.id);
        }
        Some("Turn") => {
            let Some(t) = parsed.transcript else {
                return;
            };
            let trimmed = t.trim();
            if trimmed.is_empty() {
                return;
            }
            // We accumulate every Turn (rolling transcript). On end_session
            // we only return the LAST turn — but Universal-3 Pro Streaming
            // turns are cumulative-replacement, so we keep the last and
            // discard earlier partials.
            {
                let mut finals = state.session_finals.lock();
                finals.clear();
                finals.push(trimmed.to_string());
            }
            debug!("aai turn: {trimmed:?}");
            if !*state.recording.lock() {
                state.final_arrived.notify_one();
            }
        }
        Some("Termination") => {
            debug!("aai: termination");
            if !*state.recording.lock() {
                state.final_arrived.notify_one();
            }
        }
        Some(other) => trace!("aai: unhandled type {other}"),
        None => {}
    }
}
