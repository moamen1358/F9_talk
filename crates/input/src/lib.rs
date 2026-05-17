//! Hotkey listener (chord parser + push-to-talk events) and text typer.

#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]

pub mod hotkey;
pub mod typer;

pub use hotkey::{
    spawn as spawn_hotkey, spawn_with_debounce as spawn_hotkey_with_debounce, HotkeyEvent,
};
pub use typer::{preflight as typer_preflight, PreflightError, Typer};
