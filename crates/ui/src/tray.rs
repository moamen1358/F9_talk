//! System-tray icon with three visual states (active / paused / error)
//! and the right-click menu (Pause/Resume + Edit API Keys + Quit).
//!
//! On Linux the tray thread runs `gtk::main()` and uses `glib` timers
//! for state refresh. On macOS / Windows we use a simple polling loop
//! instead (no GTK dependency).

use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

use crate::indicator::IndicatorState;

/// Bundled brand icon. Source at the workspace root `assets/` directory;
/// cargo-deb copies it to `/usr/share/icons/hicolor/` at install time.
const ICON_BYTES: &[u8] = include_bytes!("../../../assets/f9-talk.png");

#[derive(Debug, Clone)]
pub enum TrayCommand {
    PauseToggled(bool),
    EditKeys,
    Quit,
}

#[derive(Debug, Clone)]
pub struct VisualState {
    pub paused: bool,
    pub error: bool,
}

/// Public handle returned to the app for runtime mutations. All
/// methods are non-blocking: they update an `Arc<Mutex<…>>` that the
/// tray thread reads in a periodic callback. Worst-case lag ≈ 50 ms.
#[derive(Clone)]
pub struct TrayHandle {
    state: Arc<Mutex<VisualState>>,
    dirty: Arc<Mutex<bool>>,
}

impl TrayHandle {
    pub fn set_paused(&self, paused: bool) {
        let mut s = self.state.lock();
        if s.paused != paused {
            s.paused = paused;
            *self.dirty.lock() = true;
        }
    }
    pub fn set_error(&self, error: bool) {
        let mut s = self.state.lock();
        if s.error != error {
            s.error = error;
            *self.dirty.lock() = true;
        }
    }
}

/// Spawn the tray on a dedicated thread. Returns the handle for state
/// mutations + a receiver the app uses to drive STT/UI commands.
pub fn spawn(
    initial: VisualState,
    indicator_state: Arc<IndicatorState>,
) -> anyhow::Result<(TrayHandle, mpsc::UnboundedReceiver<TrayCommand>)> {
    let state = Arc::new(Mutex::new(initial));
    let dirty = Arc::new(Mutex::new(true));
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<TrayCommand>();

    let state_for_thread = state.clone();
    let dirty_for_thread = dirty.clone();
    let indicator = indicator_state;
    std::thread::Builder::new()
        .name("f9-talk-tray".into())
        .spawn(move || {
            run_tray_thread(state_for_thread, dirty_for_thread, cmd_tx, indicator);
        })?;

    Ok((TrayHandle { state, dirty }, cmd_rx))
}

// ── Linux tray thread (GTK main loop) ──────────────────────────────
#[cfg(target_os = "linux")]
fn run_tray_thread(
    state: Arc<Mutex<VisualState>>,
    dirty: Arc<Mutex<bool>>,
    cmd_tx: mpsc::UnboundedSender<TrayCommand>,
    indicator: Arc<IndicatorState>,
) {
    if let Err(e) = gtk::init() {
        error!("gtk::init failed: {e}; tray will be unavailable");
        return;
    }

    let icons = match Icons::load() {
        Ok(i) => i,
        Err(e) => {
            warn!("could not decode tray icon: {e}; using a 1×1 placeholder");
            Icons::placeholder()
        }
    };

    let pause_item = CheckMenuItem::new(
        if state.lock().paused {
            "Resume listening"
        } else {
            "Pause listening"
        },
        true,
        state.lock().paused,
        None,
    );
    let keys_item = tray_icon::menu::MenuItem::new("API Keys…", true, None);
    let quit_item = tray_icon::menu::MenuItem::new("Quit", true, None);
    let sep = PredefinedMenuItem::separator();
    let menu = Menu::with_items(&[
        &pause_item as &dyn IsMenuItem,
        &sep as &dyn IsMenuItem,
        &keys_item as &dyn IsMenuItem,
        &sep as &dyn IsMenuItem,
        &quit_item as &dyn IsMenuItem,
    ])
    .expect("menu build");

    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("F9 Talk — listening")
        .with_icon(icons.active.clone())
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            error!("tray-icon build failed: {e}");
            return;
        }
    };
    info!("tray icon ready");

    let pause_id = pause_item.id().clone();
    let keys_id = keys_item.id().clone();
    let quit_id = quit_item.id().clone();
    spawn_menu_forwarder(
        cmd_tx.clone(),
        state.clone(),
        Ids {
            pause: pause_id,
            keys: keys_id,
            quit: quit_id,
        },
    );

    let state_for_tick = state.clone();
    let dirty_for_tick = dirty;
    let icons_for_tick = icons;
    let pause_for_tick = pause_item;
    let _ = glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        if !*dirty_for_tick.lock() {
            return glib::ControlFlow::Continue;
        }
        *dirty_for_tick.lock() = false;

        let s = state_for_tick.lock().clone();
        let icon = if s.error {
            &icons_for_tick.error
        } else if s.paused {
            &icons_for_tick.paused
        } else {
            &icons_for_tick.active
        };
        let _ = tray.set_icon(Some(icon.clone()));
        let tooltip = match (s.paused, s.error) {
            (true, _) => "F9 Talk — paused",
            (false, true) => "F9 Talk — last session failed",
            (false, false) => "F9 Talk — listening",
        };
        let _ = tray.set_tooltip(Some(tooltip));

        pause_for_tick.set_text(if s.paused {
            "Resume listening"
        } else {
            "Pause listening"
        });
        pause_for_tick.set_checked(s.paused);

        if s.paused {
            indicator.set_recording(false);
        }

        glib::ControlFlow::Continue
    });

    gtk::main();
}

// ── macOS / Windows tray thread (polling loop) ─────────────────────
#[cfg(not(target_os = "linux"))]
fn run_tray_thread(
    state: Arc<Mutex<VisualState>>,
    dirty: Arc<Mutex<bool>>,
    cmd_tx: mpsc::UnboundedSender<TrayCommand>,
    indicator: Arc<IndicatorState>,
) {
    let icons = match Icons::load() {
        Ok(i) => i,
        Err(e) => {
            warn!("could not decode tray icon: {e}; using a 1×1 placeholder");
            Icons::placeholder()
        }
    };

    let pause_item = CheckMenuItem::new(
        if state.lock().paused {
            "Resume listening"
        } else {
            "Pause listening"
        },
        true,
        state.lock().paused,
        None,
    );
    let keys_item = tray_icon::menu::MenuItem::new("API Keys…", true, None);
    let quit_item = tray_icon::menu::MenuItem::new("Quit", true, None);
    let sep = PredefinedMenuItem::separator();
    let menu = Menu::with_items(&[
        &pause_item as &dyn IsMenuItem,
        &sep as &dyn IsMenuItem,
        &keys_item as &dyn IsMenuItem,
        &sep as &dyn IsMenuItem,
        &quit_item as &dyn IsMenuItem,
    ])
    .expect("menu build");

    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("F9 Talk — listening")
        .with_icon(icons.active.clone())
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            error!("tray-icon build failed: {e}");
            return;
        }
    };
    info!("tray icon ready");

    let pause_id = pause_item.id().clone();
    let keys_id = keys_item.id().clone();
    let quit_id = quit_item.id().clone();
    spawn_menu_forwarder(
        cmd_tx.clone(),
        state.clone(),
        Ids {
            pause: pause_id,
            keys: keys_id,
            quit: quit_id,
        },
    );

    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));

        if !*dirty.lock() {
            continue;
        }
        *dirty.lock() = false;

        let s = state.lock().clone();
        let icon = if s.error {
            &icons.error
        } else if s.paused {
            &icons.paused
        } else {
            &icons.active
        };
        let _ = tray.set_icon(Some(icon.clone()));
        let tooltip = match (s.paused, s.error) {
            (true, _) => "F9 Talk — paused",
            (false, true) => "F9 Talk — last session failed",
            (false, false) => "F9 Talk — listening",
        };
        let _ = tray.set_tooltip(Some(tooltip));

        pause_item.set_text(if s.paused {
            "Resume listening"
        } else {
            "Pause listening"
        });
        pause_item.set_checked(s.paused);

        if s.paused {
            indicator.set_recording(false);
        }
    }
}

struct Ids {
    pause: MenuId,
    keys: MenuId,
    quit: MenuId,
}

fn spawn_menu_forwarder(
    cmd_tx: mpsc::UnboundedSender<TrayCommand>,
    state: Arc<Mutex<VisualState>>,
    ids: Ids,
) {
    std::thread::Builder::new()
        .name("f9-talk-tray-events".into())
        .spawn(move || {
            let rx = MenuEvent::receiver();
            while let Ok(evt) = rx.recv() {
                let id = evt.id();
                let cmd = if id == &ids.pause {
                    let new_paused = !state.lock().paused;
                    Some(TrayCommand::PauseToggled(new_paused))
                } else if id == &ids.keys {
                    Some(TrayCommand::EditKeys)
                } else if id == &ids.quit {
                    Some(TrayCommand::Quit)
                } else {
                    debug!("unknown menu event id: {id:?}");
                    None
                };
                if let Some(cmd) = cmd {
                    if cmd_tx.send(cmd).is_err() {
                        return;
                    }
                }
            }
        })
        .expect("spawn tray-events forwarder");
}

struct Icons {
    active: Icon,
    paused: Icon,
    error: Icon,
}

impl Icons {
    fn load() -> anyhow::Result<Self> {
        let img = image::load_from_memory(ICON_BYTES)?.to_rgba8();
        let (w, h) = img.dimensions();
        let small = image::imageops::resize(&img, 32, 32, image::imageops::FilterType::Lanczos3);
        let active = Icon::from_rgba(small.clone().into_raw(), 32, 32)?;
        let paused = Icon::from_rgba(desaturate(&small).into_raw(), 32, 32)?;
        let error = Icon::from_rgba(red_tint(&small).into_raw(), 32, 32)?;
        debug!("tray icons built from bundled asset ({}×{})", w, h);
        Ok(Self {
            active,
            paused,
            error,
        })
    }

    fn placeholder() -> Self {
        let pixel = vec![230, 60, 80, 255];
        Self {
            active: Icon::from_rgba(pixel.clone(), 1, 1).unwrap(),
            paused: Icon::from_rgba(pixel.clone(), 1, 1).unwrap(),
            error: Icon::from_rgba(pixel, 1, 1).unwrap(),
        }
    }
}

fn desaturate(rgba: &image::ImageBuffer<image::Rgba<u8>, Vec<u8>>) -> image::RgbaImage {
    let mut out = rgba.clone();
    for p in out.pixels_mut() {
        let [r, g, b, a] = p.0;
        let gray = (0.30 * r as f32 + 0.59 * g as f32 + 0.11 * b as f32) as u8;
        let a = (a as f32 * 0.5) as u8;
        p.0 = [gray, gray, gray, a];
    }
    out
}

fn red_tint(rgba: &image::ImageBuffer<image::Rgba<u8>, Vec<u8>>) -> image::RgbaImage {
    let mut out = rgba.clone();
    for p in out.pixels_mut() {
        let [r, _g, _b, a] = p.0;
        p.0 = [r.saturating_add(60), 30, 40, a];
    }
    out
}
