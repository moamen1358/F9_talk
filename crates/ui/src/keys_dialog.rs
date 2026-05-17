//! API Keys dialog. Opens as a separate egui viewport when the user
//! picks "API Keys…" from the tray. Saves to a platform-appropriate
//! config path (e.g. `~/.config/F9_talk/secrets.env` on Linux).
//!
//! Single row (Deepgram Nova-3) with a Show/Hide toggle + Save/Cancel.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tracing::info;

const FRAME_WIDTH: f32 = 460.0;
const FRAME_HEIGHT: f32 = 190.0;

/// Saved keys handed off to the session loop after the user clicks Save.
#[derive(Debug, Clone)]
pub struct KeysSaved {
    /// Some(value) if the user changed the Deepgram key (may be empty
    /// to clear it). None means "leave as-is".
    pub deepgram: Option<String>,
}

#[derive(Default)]
struct Inner {
    open: bool,
    dg: String,
    dg_show: bool,
    dg_initial: String,
    pending_save: Option<KeysSaved>,
}

#[derive(Clone, Default)]
pub struct KeysDialogState {
    inner: Arc<Mutex<Inner>>,
}

impl KeysDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the dialog with the current secrets pre-filled. Called from
    /// the tokio session loop on TrayCommand::EditKeys.
    pub fn open(&self, current_dg: String) {
        let mut s = self.inner.lock();
        s.dg_initial = current_dg.clone();
        s.dg = current_dg;
        s.dg_show = false;
        s.pending_save = None;
        s.open = true;
    }

    pub fn is_open(&self) -> bool {
        self.inner.lock().open
    }

    /// Pick up any pending save (consumed once). Called from the
    /// session loop's tokio_select! arm in a poll loop.
    pub fn take_pending_save(&self) -> Option<KeysSaved> {
        self.inner.lock().pending_save.take()
    }

    /// Render the form. Called from inside the dialog's deferred viewport.
    fn render(&self, ui: &mut egui::Ui) -> DialogAction {
        let mut action = DialogAction::None;
        let mut s = self.inner.lock();

        ui.heading("API Keys");
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Stored at ~/.config/F9_talk/secrets.env — never sent anywhere except Deepgram.",
            )
            .size(11.0)
            .color(egui::Color32::from_gray(140)),
        );
        ui.add_space(12.0);

        egui::Grid::new("keys-grid")
            .num_columns(3)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                let dg_show = s.dg_show;

                ui.label("Deepgram");
                ui.add(
                    egui::TextEdit::singleline(&mut s.dg)
                        .desired_width(280.0)
                        .password(!dg_show)
                        .hint_text("paste your Deepgram key…"),
                );
                if ui.button(if dg_show { "Hide" } else { "Show" }).clicked() {
                    s.dg_show = !dg_show;
                }
                ui.end_row();
            });

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                s.open = false;
                action = DialogAction::Close;
            }
            ui.add_space(8.0);
            if ui.button("Save").clicked() {
                let dg_changed = s.dg != s.dg_initial;
                let saved = KeysSaved {
                    deepgram: dg_changed.then_some(s.dg.clone()),
                };
                s.pending_save = Some(saved);
                s.open = false;
                action = DialogAction::Close;
            }
        });

        action
    }
}

#[derive(PartialEq, Eq)]
enum DialogAction {
    None,
    Close,
}

/// Show the dialog viewport from inside an `eframe::App::update` call.
/// Renders nothing if the dialog is not open.
pub fn maybe_show_dialog(ctx: &egui::Context, state: &KeysDialogState) {
    if !state.is_open() {
        return;
    }
    let state_for_viewport = state.clone();
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("f9-talk-keys-dialog"),
        egui::ViewportBuilder::default()
            .with_title("F9 Talk — API Keys")
            .with_inner_size([FRAME_WIDTH, FRAME_HEIGHT])
            .with_resizable(false),
        move |ctx, _class| {
            // Honour the OS close button.
            if ctx.input(|i| i.viewport().close_requested()) {
                state_for_viewport.inner.lock().open = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                let action = state_for_viewport.render(ui);
                if action == DialogAction::Close {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        },
    );
}

/// Path resolver matching the loader in app/main.rs.
pub fn secrets_path() -> Option<PathBuf> {
    let config = dirs::config_dir()?;
    Some(config.join("F9_talk").join("secrets.env"))
}

/// Persist the changed key to `secrets.env`, preserving comments,
/// blank lines, and any other keys (Gladia, MyMemory, etc.) untouched.
/// Atomic write via a tmp + rename. Permissions clamped to 0600.
pub fn save_to_disk(saved: &KeysSaved) -> std::io::Result<()> {
    let Some(path) = secrets_path() else {
        return Err(std::io::Error::other("HOME not set"));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let mut out = String::new();
    let mut saw_dg = false;
    for raw_line in existing.lines() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("DEEPGRAM_API_KEY=") || trimmed.starts_with("DEEPGRAM_API_KEY ") {
            saw_dg = true;
            if let Some(v) = saved.deepgram.as_ref() {
                out.push_str(&format!("DEEPGRAM_API_KEY={v}\n"));
                continue;
            }
        }
        out.push_str(raw_line);
        out.push('\n');
    }
    if !saw_dg {
        if let Some(v) = saved.deepgram.as_ref() {
            out.push_str(&format!("DEEPGRAM_API_KEY={v}\n"));
        }
    }

    let tmp = path.with_extension("env.tmp");
    std::fs::write(&tmp, &out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, &path)?;
    info!(
        "secrets.env updated (dg_changed={})",
        saved.deepgram.is_some()
    );
    Ok(())
}

/// Best-effort: parse the secrets.env to find the current Deepgram value
/// for pre-fill. Mirrors `app::load_secrets` but only extracts the key
/// the dialog cares about.
pub fn read_current_keys() -> String {
    let Some(path) = secrets_path() else {
        return String::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return String::new();
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("DEEPGRAM_API_KEY=") {
            return v.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }
    String::new()
}
