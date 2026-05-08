//! Direct `/dev/uinput` typer. Opens the uinput device once at start and
//! reuses it; falls back to the `ydotool` subprocess only when the
//! `ydotool-fallback` feature is enabled at build time.
//!
//! Status: stub for M1 wiring.
