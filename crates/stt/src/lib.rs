//! STT backend trait + concrete AssemblyAI, Deepgram, and local Whisper impls.
//!
//! Every backend implements [`Stt`] (M2 work). v1 wraps:
//! - AssemblyAI Universal-3 Pro Streaming (cloud, default)
//! - Deepgram Nova-3 (cloud, alt)
//! - whisper-rs / whisper.cpp (local; gated behind the `cuda` feature)
//!
//! Status: stub for M1 wiring.

#![forbid(unsafe_code)]
