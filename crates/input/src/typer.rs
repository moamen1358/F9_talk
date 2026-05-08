//! Direct `/dev/uinput` typer: builds a virtual keyboard at construction,
//! reuses it for every press, and emits scancode press/release pairs with
//! `evdev::uinput::VirtualDevice`.
//!
//! ASCII printable text maps directly to (key, shift?). Non-ASCII chars
//! use the IBus Ctrl+Shift+U + hex + space sequence which works on every
//! modern Linux desktop with IBus or fcitx running (Pop!_OS default).
//!
//! `/dev/uinput` is mode 0600 root:root by default. Ship the
//! `packaging/debian/udev/99-f9-talk.rules` file (KERNEL=="uinput",
//! MODE="0660", GROUP="input") to grant the input group write access.

use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use tracing::{debug, info, warn};

const UINPUT_DEV: &str = "/dev/uinput";

/// Pre-type sleep matching Python's `Typer.type_text()` — lets the
/// hotkey fully release before any key event lands.
pub const PRE_TYPE_SLEEP: Duration = Duration::from_millis(80);

/// Inter-keystroke pause. Some text-entry widgets (especially web inputs
/// with input-validation listeners) drop characters when a long string
/// arrives faster than ~5 ms/char. Matches xdotool's typical effective
/// rate at `--delay 0`.
const KEY_DELAY: Duration = Duration::from_micros(2_500);

#[derive(thiserror::Error, Debug)]
pub enum PreflightError {
    #[error("/dev/uinput not present — install kernel module 'uinput' and reboot")]
    MissingDevice,
    #[error(
        "/dev/uinput is not writable by the current user — \
        run `sudo usermod -aG input $USER`, install the udev rule \
        in packaging/debian/udev/99-f9-talk.rules, then log out and back in once"
    )]
    NotWritable,
}

/// Soft preflight: returns Ok with a warn-log instead of failing, since
/// the hard error surfaces inside `Typer::new` with a clearer message.
pub fn preflight() -> Result<(), PreflightError> {
    let dev = Path::new(UINPUT_DEV);
    if !dev.exists() {
        warn!("preflight: {} is missing", UINPUT_DEV);
        return Ok(());
    }
    match std::fs::OpenOptions::new().write(true).open(dev) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            warn!(
                "preflight: {} is not writable yet — \
                run `sudo usermod -aG input $USER` and install the udev rule \
                (`packaging/debian/udev/99-f9-talk.rules`), then log out + in",
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

pub struct Typer {
    device: VirtualDevice,
}

impl Typer {
    pub fn new() -> anyhow::Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for k in EVERY_KEY_WE_USE {
            keys.insert(*k);
        }

        let device = VirtualDevice::builder()
            .map_err(|e| anyhow::anyhow!("could not open /dev/uinput: {e}"))?
            .name("f9-talk virtual keyboard")
            .with_keys(&keys)
            .map_err(|e| anyhow::anyhow!("uinput with_keys failed: {e}"))?
            .build()
            .map_err(|e| anyhow::anyhow!("uinput build failed: {e}"))?;

        // The kernel needs a tiny moment between device creation and
        // first event before Xorg / Wayland recognise the new keyboard.
        sleep(Duration::from_millis(120));

        info!("uinput typer ready (virtual device: 'f9-talk virtual keyboard')");
        Ok(Typer { device })
    }

    pub fn type_text(&mut self, text: &str) -> anyhow::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        sleep(PRE_TYPE_SLEEP);
        for c in text.chars() {
            if c == '\r' {
                continue;
            }
            if let Some((key, needs_shift)) = ascii_char_to_key(c) {
                self.tap(key, needs_shift)?;
            } else {
                self.type_unicode(c as u32)?;
            }
            sleep(KEY_DELAY);
        }
        Ok(())
    }

    fn tap(&mut self, key: KeyCode, needs_shift: bool) -> anyhow::Result<()> {
        let mut events: Vec<InputEvent> = Vec::with_capacity(4);
        if needs_shift {
            events.push(key_event(KeyCode::KEY_LEFTSHIFT, 1));
        }
        events.push(key_event(key, 1));
        events.push(key_event(key, 0));
        if needs_shift {
            events.push(key_event(KeyCode::KEY_LEFTSHIFT, 0));
        }
        self.device.emit(&events)?;
        Ok(())
    }

    fn type_unicode(&mut self, codepoint: u32) -> anyhow::Result<()> {
        // IBus / GTK Ctrl+Shift+U dance: Ctrl+Shift+U, hex digits, space.
        let hex = format!("{codepoint:x}");
        debug!("typing unicode U+{hex} via Ctrl+Shift+U");

        // Press Ctrl+Shift+U
        self.device.emit(&[
            key_event(KeyCode::KEY_LEFTCTRL, 1),
            key_event(KeyCode::KEY_LEFTSHIFT, 1),
            key_event(KeyCode::KEY_U, 1),
            key_event(KeyCode::KEY_U, 0),
            key_event(KeyCode::KEY_LEFTSHIFT, 0),
            key_event(KeyCode::KEY_LEFTCTRL, 0),
        ])?;
        sleep(Duration::from_millis(5));

        // Hex digits
        for c in hex.chars() {
            if let Some((key, _shift)) = ascii_char_to_key(c) {
                self.tap(key, false)?;
                sleep(KEY_DELAY);
            }
        }

        // Space to commit
        self.tap(KeyCode::KEY_SPACE, false)?;
        Ok(())
    }
}

fn key_event(key: KeyCode, value: i32) -> InputEvent {
    InputEvent::new(EventType::KEY.0, key.code(), value)
}

/// Map an ASCII printable char to (KeyCode, needs_shift). For everything
/// else, returns `None` — caller should fall back to the Unicode path.
fn ascii_char_to_key(c: char) -> Option<(KeyCode, bool)> {
    match c {
        'a'..='z' => Some((
            KeyCode(KeyCode::KEY_A.code() + (c as u16 - b'a' as u16)),
            false,
        )),
        'A'..='Z' => Some((
            KeyCode(KeyCode::KEY_A.code() + (c.to_ascii_lowercase() as u16 - b'a' as u16)),
            true,
        )),
        '0' => Some((KeyCode::KEY_0, false)),
        '1'..='9' => Some((
            KeyCode(KeyCode::KEY_1.code() + (c as u16 - b'1' as u16)),
            false,
        )),
        ' ' => Some((KeyCode::KEY_SPACE, false)),
        '\n' => Some((KeyCode::KEY_ENTER, false)),
        '\t' => Some((KeyCode::KEY_TAB, false)),
        '.' => Some((KeyCode::KEY_DOT, false)),
        ',' => Some((KeyCode::KEY_COMMA, false)),
        ';' => Some((KeyCode::KEY_SEMICOLON, false)),
        '\'' => Some((KeyCode::KEY_APOSTROPHE, false)),
        '"' => Some((KeyCode::KEY_APOSTROPHE, true)),
        '/' => Some((KeyCode::KEY_SLASH, false)),
        '?' => Some((KeyCode::KEY_SLASH, true)),
        '\\' => Some((KeyCode::KEY_BACKSLASH, false)),
        '|' => Some((KeyCode::KEY_BACKSLASH, true)),
        '-' => Some((KeyCode::KEY_MINUS, false)),
        '_' => Some((KeyCode::KEY_MINUS, true)),
        '=' => Some((KeyCode::KEY_EQUAL, false)),
        '+' => Some((KeyCode::KEY_EQUAL, true)),
        '!' => Some((KeyCode::KEY_1, true)),
        '@' => Some((KeyCode::KEY_2, true)),
        '#' => Some((KeyCode::KEY_3, true)),
        '$' => Some((KeyCode::KEY_4, true)),
        '%' => Some((KeyCode::KEY_5, true)),
        '^' => Some((KeyCode::KEY_6, true)),
        '&' => Some((KeyCode::KEY_7, true)),
        '*' => Some((KeyCode::KEY_8, true)),
        '(' => Some((KeyCode::KEY_9, true)),
        ')' => Some((KeyCode::KEY_0, true)),
        '[' => Some((KeyCode::KEY_LEFTBRACE, false)),
        ']' => Some((KeyCode::KEY_RIGHTBRACE, false)),
        '{' => Some((KeyCode::KEY_LEFTBRACE, true)),
        '}' => Some((KeyCode::KEY_RIGHTBRACE, true)),
        '`' => Some((KeyCode::KEY_GRAVE, false)),
        '~' => Some((KeyCode::KEY_GRAVE, true)),
        '<' => Some((KeyCode::KEY_COMMA, true)),
        '>' => Some((KeyCode::KEY_DOT, true)),
        ':' => Some((KeyCode::KEY_SEMICOLON, true)),
        _ => None,
    }
}

/// All keycodes our typer might emit, registered with the virtual device
/// at construction. Includes A-Z, 0-9, every punctuation char in
/// [`ascii_char_to_key`], the modifiers we use, and the navigation keys
/// `Ctrl+Shift+U` needs.
const EVERY_KEY_WE_USE: &[KeyCode] = &[
    // Letters A-Z (KEY_A through KEY_Z; KEY_A is 30, contiguous)
    KeyCode::KEY_A,
    KeyCode::KEY_B,
    KeyCode::KEY_C,
    KeyCode::KEY_D,
    KeyCode::KEY_E,
    KeyCode::KEY_F,
    KeyCode::KEY_G,
    KeyCode::KEY_H,
    KeyCode::KEY_I,
    KeyCode::KEY_J,
    KeyCode::KEY_K,
    KeyCode::KEY_L,
    KeyCode::KEY_M,
    KeyCode::KEY_N,
    KeyCode::KEY_O,
    KeyCode::KEY_P,
    KeyCode::KEY_Q,
    KeyCode::KEY_R,
    KeyCode::KEY_S,
    KeyCode::KEY_T,
    KeyCode::KEY_U,
    KeyCode::KEY_V,
    KeyCode::KEY_W,
    KeyCode::KEY_X,
    KeyCode::KEY_Y,
    KeyCode::KEY_Z,
    // Digits
    KeyCode::KEY_0,
    KeyCode::KEY_1,
    KeyCode::KEY_2,
    KeyCode::KEY_3,
    KeyCode::KEY_4,
    KeyCode::KEY_5,
    KeyCode::KEY_6,
    KeyCode::KEY_7,
    KeyCode::KEY_8,
    KeyCode::KEY_9,
    // Whitespace + structural
    KeyCode::KEY_SPACE,
    KeyCode::KEY_ENTER,
    KeyCode::KEY_TAB,
    KeyCode::KEY_BACKSPACE,
    // Punctuation
    KeyCode::KEY_DOT,
    KeyCode::KEY_COMMA,
    KeyCode::KEY_SEMICOLON,
    KeyCode::KEY_APOSTROPHE,
    KeyCode::KEY_SLASH,
    KeyCode::KEY_BACKSLASH,
    KeyCode::KEY_MINUS,
    KeyCode::KEY_EQUAL,
    KeyCode::KEY_LEFTBRACE,
    KeyCode::KEY_RIGHTBRACE,
    KeyCode::KEY_GRAVE,
    // Modifiers
    KeyCode::KEY_LEFTSHIFT,
    KeyCode::KEY_RIGHTSHIFT,
    KeyCode::KEY_LEFTCTRL,
    KeyCode::KEY_RIGHTCTRL,
    KeyCode::KEY_LEFTALT,
    KeyCode::KEY_RIGHTALT,
];
