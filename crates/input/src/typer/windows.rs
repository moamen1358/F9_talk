//! Windows text injection: clipboard + Ctrl+V via SendInput.

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
            "clipboard+Ctrl+V"
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
            anyhow::bail!("no clipboard available for text injection on Windows");
        };

        cb.set_text(text)
            .map_err(|e| anyhow::anyhow!("clipboard set_text failed: {e}"))?;
        debug!(
            "clipboard set ({} chars); sending Ctrl+V via SendInput",
            text.len()
        );

        sleep(std::time::Duration::from_millis(50));

        send_ctrl_v();
        Ok(())
    }
}

/// Simulate Ctrl+V via the Win32 SendInput API.
fn send_ctrl_v() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    let mut inputs: [INPUT; 4] = unsafe { std::mem::zeroed() };

    // Ctrl down
    inputs[0].r#type = INPUT_KEYBOARD;
    inputs[0].Anonymous = INPUT_0 {
        ki: KEYBDINPUT {
            wVk: VK_CONTROL,
            wScan: 0,
            dwFlags: 0,
            time: 0,
            dwExtraInfo: 0,
        },
    };

    // V down
    inputs[1].r#type = INPUT_KEYBOARD;
    inputs[1].Anonymous = INPUT_0 {
        ki: KEYBDINPUT {
            wVk: VK_V,
            wScan: 0,
            dwFlags: 0,
            time: 0,
            dwExtraInfo: 0,
        },
    };

    // V up
    inputs[2].r#type = INPUT_KEYBOARD;
    inputs[2].Anonymous = INPUT_0 {
        ki: KEYBDINPUT {
            wVk: VK_V,
            wScan: 0,
            dwFlags: KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        },
    };

    // Ctrl up
    inputs[3].r#type = INPUT_KEYBOARD;
    inputs[3].Anonymous = INPUT_0 {
        ki: KEYBDINPUT {
            wVk: VK_CONTROL,
            wScan: 0,
            dwFlags: KEYEVENTF_KEYUP,
            time: 0,
            dwExtraInfo: 0,
        },
    };

    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
    debug!("SendInput Ctrl+V sent");
}
