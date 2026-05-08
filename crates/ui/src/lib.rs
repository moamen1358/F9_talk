//! eframe/egui indicator (wave animation) + tray icon + keys dialog.
//!
//! Status: M3 in progress.

#![forbid(unsafe_code)]

pub mod indicator;
pub mod keys_dialog;
pub mod positioning;
pub mod tray;

pub use indicator::{IndicatorApp, IndicatorState};
pub use positioning::Positioner;
pub use tray::{
    CloudProvider as TrayCloudProvider, TrayCommand, TrayHandle, VisualState as TrayVisualState,
};
