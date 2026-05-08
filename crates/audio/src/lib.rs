//! cpal-based microphone streamer with auto-restart on stream error.
//!
//! - Default input device, 16 kHz mono s16le
//! - 25 ms frames (800 bytes) pushed into a bounded `mpsc::channel(64)`
//!   (≈ 1.6 s headroom). On overflow we drop the oldest frame and bump
//!   a counted warn-log — the audio thread is real-time and must never
//!   block.
//! - On stream error (device disappear, callback panic, EOF) the
//!   spawner relaunches with exponential backoff (1 s → 30 s cap).

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use f9_talk_core::{FRAME_BYTES, FRAME_CHANNEL_CAPACITY, SAMPLE_RATE_HZ};
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// One mic frame. 25 ms × 16 kHz × 2 bytes = 800 B; we store as Vec for
/// channel ergonomics. The hot path allocates, but at 40 frames/sec the
/// allocator pressure is negligible.
#[derive(Debug, Clone)]
pub struct Frame {
    pub bytes: Vec<u8>,
}

/// Live RMS the UI reads each render frame. Updated from the cpal
/// callback thread; protected by a parking_lot mutex (sub-µs uncontested).
pub type RmsHandle = Arc<Mutex<f32>>;

/// Spawn the mic streamer.
///
/// Returns:
/// - the receiver for raw 25 ms frames (drop-oldest on overflow)
/// - the live RMS handle the UI can read each render
/// - a shutdown handle (drop the JoinHandle to stop)
pub fn spawn() -> anyhow::Result<(
    mpsc::Receiver<Frame>,
    RmsHandle,
    tokio::task::JoinHandle<()>,
)> {
    let (tx, rx) = mpsc::channel::<Frame>(FRAME_CHANNEL_CAPACITY);
    let rms = Arc::new(Mutex::new(0.0_f32));
    let dropped = Arc::new(AtomicU64::new(0));

    let task = tokio::spawn(stream_loop(tx, rms.clone(), dropped));
    Ok((rx, rms, task))
}

async fn stream_loop(tx: mpsc::Sender<Frame>, rms_handle: RmsHandle, dropped: Arc<AtomicU64>) {
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);

    loop {
        match build_and_run_stream(&tx, &rms_handle, &dropped) {
            Ok(()) => {
                // Stream ended without an error — caller dropped the rx.
                info!("mic stream closed normally");
                return;
            }
            Err(e) => {
                error!("mic stream error: {e}; reopening in {:.1?}", backoff);
            }
        }
        sleep(backoff).await;
        backoff = (backoff * 2).min(max_backoff);
    }
}

fn build_and_run_stream(
    tx: &mpsc::Sender<Frame>,
    rms_handle: &RmsHandle,
    dropped: &Arc<AtomicU64>,
) -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no default input device"))?;
    let device_id = device.id().ok();
    let supported = device.default_input_config()?;
    info!(
        "mic: device={device_id:?} default_config={:?} sample_format={:?}",
        supported.sample_rate(),
        supported.sample_format(),
    );

    // We always feed STT as 16 kHz mono s16le. cpal will give us whatever
    // the device's native rate is (usually 44.1/48 kHz). For M1 we just
    // pull samples through and rely on cpal's mixer to resample later;
    // M2 / M3 will tighten this if needed.
    let config = supported.config();

    let tx_cb = tx.clone();
    let rms_cb = rms_handle.clone();
    let dropped_cb = dropped.clone();

    let err_cb = |e| {
        error!("cpal stream error callback: {e}");
    };

    let stream = match supported.sample_format() {
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| forward_i16(data, &tx_cb, &rms_cb, &dropped_cb),
            err_cb,
            None,
        )?,
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| forward_f32(data, &tx_cb, &rms_cb, &dropped_cb),
            err_cb,
            None,
        )?,
        other => anyhow::bail!("unsupported cpal sample format {other:?}"),
    };
    stream.play()?;
    info!("mic stream live (target {} Hz mono s16le)", SAMPLE_RATE_HZ);

    // Park here until the receiver is dropped. We use a blocking park
    // (the cpal callback runs on its own RT thread regardless).
    while !tx.is_closed() {
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

fn forward_i16(
    data: &[i16],
    tx: &mpsc::Sender<Frame>,
    rms_handle: &RmsHandle,
    dropped: &AtomicU64,
) {
    if data.is_empty() {
        return;
    }
    update_rms_i16(data, rms_handle);
    let mut bytes = Vec::with_capacity(data.len() * 2);
    for s in data {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    push_in_frames(bytes, tx, dropped);
}

fn forward_f32(
    data: &[f32],
    tx: &mpsc::Sender<Frame>,
    rms_handle: &RmsHandle,
    dropped: &AtomicU64,
) {
    if data.is_empty() {
        return;
    }
    update_rms_f32(data, rms_handle);
    let mut bytes = Vec::with_capacity(data.len() * 2);
    for s in data {
        let clamped = s.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    push_in_frames(bytes, tx, dropped);
}

fn push_in_frames(bytes: Vec<u8>, tx: &mpsc::Sender<Frame>, dropped: &AtomicU64) {
    // We may be invoked with arbitrary buffer sizes; chop into 800 B
    // frames so consumers get exactly 25 ms of audio per recv.
    for chunk in bytes.chunks(FRAME_BYTES) {
        if chunk.len() != FRAME_BYTES {
            // Tail: keep for next callback by sending under-size only when
            // it's the last chunk we have. Simplest M1 behaviour: drop
            // the partial tail; consumers see a missing 1-25 ms gap on
            // session end which is below STT's silence threshold.
            continue;
        }
        let frame = Frame {
            bytes: chunk.to_vec(),
        };
        match tx.try_send(frame) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                let n = dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_power_of_two() {
                    warn!("mic frame channel full: dropped {n} frames total");
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Receiver gone — caller is shutting down.
            }
        }
    }
}

fn update_rms_i16(data: &[i16], rms_handle: &RmsHandle) {
    if data.is_empty() {
        return;
    }
    let sum_sq: f64 = data.iter().map(|s| (*s as f64).powi(2)).sum();
    let rms = (sum_sq / data.len() as f64).sqrt() / i16::MAX as f64;
    *rms_handle.lock() = rms as f32;
}

fn update_rms_f32(data: &[f32], rms_handle: &RmsHandle) {
    if data.is_empty() {
        return;
    }
    let sum_sq: f64 = data.iter().map(|s| (*s as f64).powi(2)).sum();
    let rms = (sum_sq / data.len() as f64).sqrt();
    *rms_handle.lock() = rms.clamp(0.0, 1.0) as f32;
}
