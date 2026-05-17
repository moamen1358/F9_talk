//! macOS text injection: clipboard + Cmd+V via osascript.

use std::thread::sleep;

use tracing::{debug, info, warn};

use super::{PreflightError, PRE_TYPE_SLEEP};

pub fn preflight() -> Result<(), PreflightError> {
    Ok(())
}

pub struct Typer {
    clipboard: Option<arboard::Clipboard>,
}

impl Typer {
    pub fn new() -> anyhow::Result<Self> {
        let clipboard = match arboard::Clipboard::new() {
            Ok(c) => Some(c),
            Err(e) => {
                warn!("could not open clipboard ({e})");
                None
            }
        };
        let method = if clipboard.is_some() {
            "clipboard+Cmd+V"
        } else {
            "none (clipboard unavailable)"
        };
        info!("typer ready (primary={method})");
        Ok(Typer { clipboard })
    }

    pub fn type_text(&mut self, text: &str) -> anyhow::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        sleep(PRE_TYPE_SLEEP);

        let Some(cb) = self.clipboard.as_mut() else {
            anyhow::bail!("no clipboard available for text injection on macOS");
        };

        cb.set_text(text)
            .map_err(|e| anyhow::anyhow!("clipboard set_text failed: {e}"))?;
        debug!("clipboard set ({} chars); sending Cmd+V via osascript", text.len());

        sleep(std::time::Duration::from_millis(50));

        match std::process::Command::new("osascript")
            .args([
                "-e",
                r#"tell application "System Events" to keystroke "v" using command down"#,
            ])
            .status()
        {
            Ok(s) if s.success() => {
                debug!("osascript Cmd+V succeeded");
                Ok(())
            }
            Ok(s) => {
                anyhow::bail!("osascript exited non-zero ({s})")
            }
            Err(e) => {
                anyhow::bail!("osascript spawn failed: {e}")
            }
        }
    }
}
