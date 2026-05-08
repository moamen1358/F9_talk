//! System-tray icon with three visual states (active / paused / error)
//! and the right-click menu (Pause/Resume + Cloud provider radio +
//! Edit API Keys + Quit). Mirrors the Python `f9_talk/ui/tray.py`.
//!
//! Threading:
//! - Lives on a dedicated `f9-talk-tray` thread because `tray-icon` on
//!   Linux requires GTK and GTK's main loop is blocking.
//! - The tray thread runs `gtk::main()`. A small forwarder thread
//!   reads `tray_icon::menu::MenuEvent::receiver()` (a global crossbeam
//!   channel) and posts [`TrayCommand`] values into a tokio
//!   `mpsc::UnboundedSender<TrayCommand>` the app loop owns.
//! - State updates from the app (set_paused / set_error / set_provider)
//!   are forwarded into the tray thread through a glib idle-callback
//!   so all GTK calls stay on the GTK thread.

use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIconBuilder};

use crate::indicator::IndicatorState;

/// Bundled brand icon. Source at the workspace root `assets/` directory;
/// cargo-deb copies it to `/usr/share/icons/hicolor/` at install time.
const ICON_BYTES: &[u8] = include_bytes!("../../../assets/f9-talk.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    Assemblyai,
    Deepgram,
}

#[derive(Debug, Clone)]
pub enum TrayCommand {
    PauseToggled(bool),
    ProviderSelected(CloudProvider),
    EditKeys,
    Quit,
}

#[derive(Debug, Clone)]
pub struct VisualState {
    pub paused: bool,
    pub error: bool,
    pub provider: CloudProvider,
}

/// Public handle returned to the app for runtime mutations. All
/// methods are non-blocking: they update an `Arc<Mutex<…>>` that the
/// GTK thread reads in a glib timeout. Worst-case lag ≈ 50 ms.
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
    pub fn set_provider(&self, p: CloudProvider) {
        let mut s = self.state.lock();
        if s.provider != p {
            s.provider = p;
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
    let aai_item = CheckMenuItem::new(
        "AssemblyAI (Universal-3 Pro Streaming)",
        true,
        matches!(state.lock().provider, CloudProvider::Assemblyai),
        None,
    );
    let dg_item = CheckMenuItem::new(
        "Deepgram (Nova-3)",
        true,
        matches!(state.lock().provider, CloudProvider::Deepgram),
        None,
    );
    let provider_submenu = Submenu::with_items(
        "Cloud provider",
        true,
        &[&aai_item as &dyn IsMenuItem, &dg_item as &dyn IsMenuItem],
    )
    .expect("submenu build");
    let keys_item = tray_icon::menu::MenuItem::new("API Keys…", true, None);
    let quit_item = tray_icon::menu::MenuItem::new("Quit", true, None);
    let sep = PredefinedMenuItem::separator();
    let menu = Menu::with_items(&[
        &pause_item as &dyn IsMenuItem,
        &sep as &dyn IsMenuItem,
        &provider_submenu as &dyn IsMenuItem,
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

    // Forward MenuEvents (global crossbeam channel) to the tokio mpsc.
    let pause_id = pause_item.id().clone();
    let aai_id = aai_item.id().clone();
    let dg_id = dg_item.id().clone();
    let keys_id = keys_item.id().clone();
    let quit_id = quit_item.id().clone();
    spawn_menu_forwarder(
        cmd_tx.clone(),
        state.clone(),
        Ids {
            pause: pause_id,
            aai: aai_id,
            dg: dg_id,
            keys: keys_id,
            quit: quit_id,
        },
    );

    // glib timeout to refresh icon/tooltip/menu-checks when state.dirty
    // is set by the handle. Cheap (no work when `dirty == false`).
    let state_for_tick = state.clone();
    let dirty_for_tick = dirty;
    let icons_for_tick = icons;
    let pause_for_tick = pause_item;
    let aai_for_tick = aai_item;
    let dg_for_tick = dg_item;
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
        aai_for_tick.set_checked(matches!(s.provider, CloudProvider::Assemblyai));
        dg_for_tick.set_checked(matches!(s.provider, CloudProvider::Deepgram));

        // Sync indicator pause state too (so the wave doesn't paint
        // when the user hits Pause from the tray).
        if s.paused {
            indicator.set_recording(false);
        }

        glib::ControlFlow::Continue
    });

    gtk::main();
}

struct Ids {
    pause: MenuId,
    aai: MenuId,
    dg: MenuId,
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
                } else if id == &ids.aai {
                    Some(TrayCommand::ProviderSelected(CloudProvider::Assemblyai))
                } else if id == &ids.dg {
                    Some(TrayCommand::ProviderSelected(CloudProvider::Deepgram))
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
        // Resize down to 32×32 — most desktop trays render at 16-22 px.
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

/// Pixel-level desaturation matching `f9_talk/ui/tray.py:_make_paused`.
fn desaturate(rgba: &image::ImageBuffer<image::Rgba<u8>, Vec<u8>>) -> image::RgbaImage {
    let mut out = rgba.clone();
    for p in out.pixels_mut() {
        let [r, g, b, a] = p.0;
        let gray = (0.30 * r as f32 + 0.59 * g as f32 + 0.11 * b as f32) as u8;
        let a = (a as f32 * 0.5) as u8; // 50% opacity
        p.0 = [gray, gray, gray, a];
    }
    out
}

/// Pixel-level red-tint matching `f9_talk/ui/tray.py:_make_error`.
fn red_tint(rgba: &image::ImageBuffer<image::Rgba<u8>, Vec<u8>>) -> image::RgbaImage {
    let mut out = rgba.clone();
    for p in out.pixels_mut() {
        let [r, _g, _b, a] = p.0;
        // Push toward red; keep alpha for shape preservation.
        p.0 = [r.saturating_add(60), 30, 40, a];
    }
    out
}
