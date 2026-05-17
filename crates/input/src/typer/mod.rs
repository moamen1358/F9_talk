//! Cross-platform text injection.
//!
//! Each platform has its own optimal strategy:
//!
//! **Linux** (primary -> fallback):
//! 1. `xdotool type` -- keysym-level, layout-independent
//! 2. Clipboard + Ctrl+V via uinput
//! 3. Direct uinput scancode synthesis (en-US only)
//!
//! **macOS**:
//! 1. Clipboard + Cmd+V via osascript
//!
//! **Windows**:
//! 1. Clipboard + Ctrl+V via SendInput

use std::time::Duration;

/// Pre-type sleep -- lets the hotkey fully release before any key event lands.
pub const PRE_TYPE_SLEEP: Duration = Duration::from_millis(80);

#[derive(thiserror::Error, Debug)]
pub enum PreflightError {
    #[cfg(target_os = "linux")]
    #[error("/dev/uinput not present -- install kernel module 'uinput' and reboot")]
    MissingDevice,
    #[cfg(target_os = "linux")]
    #[error(
        "/dev/uinput is not writable by the current user -- \
        run `sudo usermod -aG input $USER`, install the udev rule \
        in packaging/debian/udev/99-f9-talk.rules, then log out and back in once"
    )]
    NotWritable,
}

// ── Linux ──────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{preflight, Typer};

// ── macOS ──────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{preflight, Typer};

// ── Windows ────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{preflight, Typer};
