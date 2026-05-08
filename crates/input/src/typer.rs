//! Direct `/dev/uinput` typer.
//!
//! M1 status: **stub**. The exit criterion only requires logging
//! "would type: [N]" — we open and verify the uinput device exists so
//! the permission preflight is correct, but the actual `type_text`
//! method just logs. Real Unicode-aware typing arrives in M2 alongside
//! STT integration (the choice is between in-process scancode synthesis
//! and the IBus Ctrl+Shift+U Unicode path; both are decidedly post-M1).

use std::path::Path;
use std::time::Duration;

use tracing::{info, warn};

const UINPUT_DEV: &str = "/dev/uinput";

/// Pre-type sleep matching the Python `Typer.type_text()` behaviour.
/// Lets the hotkey fully release before the typer "presses" anything.
pub const PRE_TYPE_SLEEP: Duration = Duration::from_millis(80);

#[derive(thiserror::Error, Debug)]
pub enum PreflightError {
    #[error("/dev/uinput not present — install kernel module 'uinput' and reboot")]
    MissingDevice,
    #[error(
        "/dev/uinput is not writable by the current user — \
        run `sudo usermod -aG input $USER` then log out and back in once"
    )]
    NotWritable,
}

/// Verify we can open `/dev/uinput` for write.
///
/// In M1 the typer is a stub (logs "would type") so this is **soft** —
/// it logs a warning if the device is unavailable but doesn't abort.
/// M2 hardens this to an exit-2 with the actionable message once the
/// typer actually opens uinput.
pub fn preflight() -> Result<(), PreflightError> {
    let dev = Path::new(UINPUT_DEV);
    if !dev.exists() {
        warn!(
            "preflight: {} is missing — install the kernel uinput module before M2 typer lands",
            UINPUT_DEV
        );
        return Ok(());
    }
    match std::fs::OpenOptions::new().write(true).open(dev) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            warn!(
                "preflight: {} is not writable yet — \
                run `sudo usermod -aG input $USER` and ship a udev rule \
                (KERNEL==\"uinput\", MODE=\"0660\", GROUP=\"input\") \
                before M2 starts using it",
                UINPUT_DEV
            );
            Ok(())
        }
        Err(e) => {
            warn!("preflight: unexpected open error on {}: {e}", UINPUT_DEV);
            Ok(())
        }
    }
}

/// M1 stub. Logs the intended typed text instead of actually typing it.
#[derive(Debug, Default)]
pub struct Typer;

impl Typer {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Typer)
    }

    /// Type a string into the focused window. M1 stub: logs only.
    pub fn type_text(&mut self, text: &str) -> anyhow::Result<()> {
        info!(target: "f9_talk::typer", "would type: {text:?}");
        Ok(())
    }
}
