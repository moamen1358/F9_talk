//! eframe/egui indicator (wave animation) + tray icon + keys dialog.
//!
//! Status: M3 in progress.

#![forbid(unsafe_code)]

pub mod indicator;
pub mod keys_dialog;
pub mod tray;

pub use indicator::{IndicatorApp, IndicatorState};
