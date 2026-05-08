//! Session state machine, audio-frame routing, hotkey debounce.
//!
//! Every public type here is decoupled from concrete I/O: the input, audio,
//! stt, and ui crates plug into the channels this crate exposes.

#![forbid(unsafe_code)]

/// 16 kHz mono s16le PCM frame size: 25 ms × 16 kHz × 2 bytes/sample = 800 B.
pub const FRAME_BYTES: usize = 800;
pub const SAMPLE_RATE_HZ: u32 = 16_000;
pub const FRAME_MS: u32 = 25;

/// Maximum buffered audio frames between cpal callback and tokio consumers.
/// 64 × 25 ms ≈ 1.6 s headroom; older frames get dropped on overflow.
pub const FRAME_CHANNEL_CAPACITY: usize = 64;
