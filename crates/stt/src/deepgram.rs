//! Deepgram Nova-3 streaming over `tokio-tungstenite`.
//!
//! Mirrors the auto-reconnect logic shipped in v0.3.1
//! (`f9_talk/stt/deepgram.py:_reconnect_loop`):
//! - Persistent WS, kept alive across many F9 presses.
//! - Auto-reconnect on close + on three consecutive send failures.
//! - `stop()` sets a shutdown flag so clean teardown won't trigger reconnect spam.
//!
//! Wire protocol (Listen WebSocket API):
//! - URL: `wss://api.deepgram.com/v1/listen?<params>`
//! - Auth: `Authorization: Token <key>` header
//! - Send: raw int16 PCM bytes as binary WS frames
//! - Send: `{"type":"Finalize"}` text frame to force the server to emit its final transcript
//! - Send: `{"type":"KeepAlive"}` text frame periodically (every 8 s)
//! - Receive: JSON `{ "type": "Results", "is_final": true|false,
//!                    "channel": { "alternatives": [{ "transcript": "..." }] } }`

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{http::HeaderValue, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{debug, info, trace, warn};
use url::Url;

use crate::{BackendEvent, SessionResult, Stt, SttError, STT_SAMPLE_RATE};

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(8);
const RECONNECT_INITIAL: Duration = Duration::from_secs(1);
const RECONNECT_CAP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Config {
    pub model: String,
    pub language: String,
    pub keywords: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "nova-3".into(),
            language: "en".into(),
            keywords: vec![],
        }
    }
}

pub struct Deepgram {
    api_key: String,
    cfg: Config,
    state: Arc<SharedState>,
    cmd_tx: Mutex<Option<mpsc::Sender<Cmd>>>,
}

struct SharedState {
    recording: Mutex<bool>,
    session_finals: Mutex<Vec<String>>,
    /// One-shot signal published by `_on_message` when a final arrives
    /// after end_session has set recording=false. Per-session: replaced
    /// at the start of every end_session() call so a late final from a
    /// previous press can never wake the next press's await early.
    final_signal: Mutex<Option<oneshot::Sender<()>>>,
    last_error: Mutex<Option<String>>,
    shutting_down: std::sync::atomic::AtomicBool,
}

enum Cmd {
    Audio(Vec<u8>),
    Finalize,
    Stop,
}

impl Deepgram {
    pub fn new(api_key: impl Into<String>, cfg: Config) -> Self {
        Self {
            api_key: api_key.into(),
            cfg,
            state: Arc::new(SharedState {
                recording: Mutex::new(false),
                session_finals: Mutex::new(Vec::new()),
                final_signal: Mutex::new(None),
                last_error: Mutex::new(None),
                shutting_down: std::sync::atomic::AtomicBool::new(false),
            }),
            cmd_tx: Mutex::new(None),
        }
    }

    fn build_url(&self) -> Result<Url, SttError> {
        let mut u = Url::parse("wss://api.deepgram.com/v1/listen")
            .map_err(|e| SttError::Internal(e.to_string()))?;
        {
            let mut q = u.query_pairs_mut();
            q.append_pair("model", &self.cfg.model);
            q.append_pair("language", &self.cfg.language);
            q.append_pair("encoding", "linear16");
            q.append_pair("sample_rate", &STT_SAMPLE_RATE.to_string());
            q.append_pair("channels", "1");
            q.append_pair("interim_results", "false");
            q.append_pair("smart_format", "true");
            q.append_pair("punctuate", "true");
            q.append_pair("endpointing", "25");
            q.append_pair("no_delay", "true");
            for kw in &self.cfg.keywords {
                q.append_pair("keywords", kw);
            }
        }
        Ok(u)
    }
}

#[async_trait]
impl Stt for Deepgram {
    fn name(&self) -> &'static str {
        "deepgram"
    }

    async fn start(&self, events: mpsc::Sender<BackendEvent>) -> Result<(), SttError> {
        if self.api_key.is_empty() {
            return Err(SttError::MissingKey("DEEPGRAM_API_KEY"));
        }
        let url = self.build_url()?;
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>(256);
        *self.cmd_tx.lock() = Some(cmd_tx);

        let api_key = self.api_key.clone();
        let state = self.state.clone();
        state
            .shutting_down
            .store(false, std::sync::atomic::Ordering::Relaxed);

        tokio::spawn(reconnect_loop(api_key, url, cmd_rx, events, state));
        Ok(())
    }

    async fn begin_session(&self) {
        *self.state.recording.lock() = true;
        self.state.session_finals.lock().clear();
        // Drop any sender from a previous end_session — late finals
        // from the prior press will find None and silently drop.
        *self.state.final_signal.lock() = None;
        *self.state.last_error.lock() = None;
    }

    async fn send_audio(&self, pcm: &[u8]) {
        if !*self.state.recording.lock() {
            return;
        }
        let Some(tx) = self.cmd_tx.lock().clone() else {
            return;
        };
        // Try-send so a stalled WS doesn't block the audio thread.
        let _ = tx.try_send(Cmd::Audio(pcm.to_vec()));
    }

    async fn end_session(&self, timeout: Duration) -> SessionResult {
        let started = Instant::now();
        // Install a fresh oneshot signal BEFORE flipping recording=false.
        // Otherwise a final that lands between recording=false and the
        // signal install would be lost (no waker).
        let (signal_tx, signal_rx) = oneshot::channel();
        *self.state.final_signal.lock() = Some(signal_tx);
        *self.state.recording.lock() = false;

        if let Some(tx) = self.cmd_tx.lock().clone() {
            let _ = tx.try_send(Cmd::Finalize);
        }

        // Wait up to `timeout` for the message handler to fire the
        // oneshot. Late finals from THIS press's audio fill in
        // session_finals; if the timeout expires first, we return
        // whatever's in there (probably empty).
        let _ = tokio::time::timeout(timeout, signal_rx).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        // Drop the slot so a late final from THIS session doesn't
        // wake the next press.
        *self.state.final_signal.lock() = None;

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
    api_key: String,
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
        match run_connection(&api_key, &url, &mut cmd_rx, &state).await {
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
                        "deepgram socket closed; reconnecting".into(),
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

        // Reconnected indicator on next successful run.
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
    api_key: &str,
    url: &Url,
    cmd_rx: &mut mpsc::Receiver<Cmd>,
    state: &Arc<SharedState>,
) -> ConnectionEnd {
    let mut req = match url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => return ConnectionEnd::Error(format!("bad request: {e}")),
    };
    let auth = match HeaderValue::from_str(&format!("Token {api_key}")) {
        Ok(v) => v,
        Err(e) => return ConnectionEnd::Error(format!("bad auth header: {e}")),
    };
    req.headers_mut().insert("Authorization", auth);

    let stream = match connect_async(req).await {
        Ok((s, _resp)) => s,
        Err(e) => return ConnectionEnd::Error(format!("connect: {e}")),
    };
    info!("Deepgram socket open (model query in URL)");

    let outcome = pump_messages(stream, cmd_rx, state).await;
    info!("Deepgram socket closed: {outcome:?}");
    outcome
}

async fn pump_messages(
    stream: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    cmd_rx: &mut mpsc::Receiver<Cmd>,
    state: &Arc<SharedState>,
) -> ConnectionEnd {
    let (mut sink, mut source) = stream.split();
    let mut keepalive_tick = tokio::time::interval(KEEPALIVE_INTERVAL);
    keepalive_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let _ = keepalive_tick.tick().await; // skip the immediate fire

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
                    Some(Cmd::Finalize) => {
                        if let Err(e) = sink.send(Message::Text(r#"{"type":"Finalize"}"#.into())).await {
                            warn!("Finalize send failed: {e}");
                        }
                    }
                    Some(Cmd::Stop) => {
                        let _ = sink.send(Message::Text(r#"{"type":"CloseStream"}"#.into())).await;
                        return ConnectionEnd::Stop;
                    }
                    None => return ConnectionEnd::Stop,
                }
            }
            msg = source.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) => handle_text(&t, state),
                    Some(Ok(Message::Binary(_))) => {} // ignored
                    Some(Ok(Message::Ping(p))) => { let _ = sink.send(Message::Pong(p)).await; }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Ok(Message::Frame(_))) | None => {
                        return ConnectionEnd::Closed;
                    }
                    Some(Err(e)) => return ConnectionEnd::Error(format!("ws read: {e}")),
                }
            }
            _ = keepalive_tick.tick() => {
                if let Err(e) = sink.send(Message::Text(r#"{"type":"KeepAlive"}"#.into())).await {
                    return ConnectionEnd::Error(format!("keepalive send: {e}"));
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct DgMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    is_final: Option<bool>,
    channel: Option<DgChannel>,
}

#[derive(Deserialize)]
struct DgChannel {
    alternatives: Vec<DgAlternative>,
}

#[derive(Deserialize)]
struct DgAlternative {
    transcript: String,
}

fn handle_text(text: &str, state: &Arc<SharedState>) {
    let parsed: DgMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            trace!("dg: non-JSON / unknown payload ({e}): {text:?}");
            return;
        }
    };
    if parsed.msg_type.as_deref() != Some("Results") {
        return;
    }
    let Some(channel) = parsed.channel else {
        return;
    };
    let Some(alt) = channel.alternatives.into_iter().next() else {
        return;
    };
    let transcript = alt.transcript.trim();
    if transcript.is_empty() {
        return;
    }
    if !parsed.is_final.unwrap_or(false) {
        return;
    }

    let mut finals = state.session_finals.lock();
    finals.push(transcript.to_string());
    drop(finals);
    debug!("dg final: {transcript:?}");

    if !*state.recording.lock() {
        // The press is over and we got a final after end_session ran:
        // wake the waiter via the per-session oneshot, if it's still
        // installed. Late finals (after end_session has already
        // returned) find None and silently drop.
        if let Some(tx) = state.final_signal.lock().take() {
            let _ = tx.send(());
        }
    }
}
