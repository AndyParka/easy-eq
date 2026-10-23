use crate::{controller::{self, Command, Controller}, editor::{self, History}, graph, instance::Instance, spectrum::Visualiser};
use eframe::egui::{self, Color32, Key, Modifiers, RichText, ViewportCommand};
use easy_eq_core::{Band, FilterKind, PanMode, Profile, workspace::{ProfileMode, Workspace}};
use std::{io::Read, path::PathBuf, sync::mpsc, time::{Duration, Instant}};
use tray_icon::{menu::{Menu, MenuEvent, MenuItem}, TrayIcon, TrayIconBuilder, TrayIconEvent};

const MUTED: Color32 = Color32::from_rgb(96, 109, 126);
const INK: Color32 = Color32::from_rgb(37, 49, 65);
const STATUS_INTERVAL: Duration = Duration::from_secs(5);

pub struct Desktop {
    controller: Controller, instance: Instance, workspace: Workspace, base: Workspace,
    sequence: u64, native_revision: u64, ear: usize, linked: bool, selected: Option<String>,
    history: History, settings_open: bool, outputs_open: bool, show_unavailable: bool,
    rename: Option<(String, String)>, delete: Option<String>, quitting: bool,
    tray_events: mpsc::Receiver<u8>, tray: Option<TrayIcon>, notice: String, close_handled: bool,
    screenshot: Option<PathBuf>, screenshot_at: Instant, hidden: bool,
    status_snapshot: controller::Snapshot, status_sequence: u64, status_notice: String, status_updated: Instant,
    visualiser: Visualiser,
    #[cfg(debug_assertions)] fixture_original: Option<Workspace>,
}
fn uid() -> String {
    format!("native-{}-{}", std::process::id(), std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos())
}
fn style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.style_mut_of(egui::Theme::Light, |style| {
        style.spacing.item_spacing = egui::vec2(6.0, 7.0);
        style.spacing.button_padding = egui::vec2(9.0, 4.0);
        style.spacing.interact_size.y = 25.0;
        style.visuals.panel_fill = Color32::from_rgb(239, 242, 246);
        style.visuals.window_fill = Color32::from_rgb(247, 249, 252);
        style.visuals.override_text_color = Some(INK);
        style.visuals.selection.bg_fill = Color32::from_rgb(205, 225, 246);
        style.visuals.selection.stroke.color = Color32::from_rgb(24, 85, 149);
        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(11.5));
    });
}
impl Desktop {
    pub fn new(cc: &eframe::CreationContext<'_>, instance: Instance, hidden: bool, screenshot: Option<PathBuf>) -> Self {
        style(&cc.egui_ctx);
        let controller = Controller::start(cc.egui_ctx.clone());
        let status_snapshot = controller.snapshot();
        let workspace = status_snapshot.workspace.clone();
        let (sender, receiver) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let menu = Menu::new();
        let open = MenuItem::with_id("open", "Open Easy EQ", true, None);
        let off = MenuItem::with_id("off", "Turn EQ off", true, None);
        let on = MenuItem::with_id("on", "Turn EQ on", true, None);
        let quit = MenuItem::with_id("quit", "Quit Easy EQ", true, None);
        let _ = menu.append_items(&[&open, &off, &on, &quit]);
        let event_sender = sender.clone(); let event_ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = match event.id.as_ref() { "open" => 0, "off" => 1, "on" => 2, "quit" => 3, _ => return };
            let _ = event_sender.send(action); event_ctx.request_repaint();
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if matches!(event, TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. }) {
                let _ = sender.send(0); ctx.request_repaint();
            }
        }));
        let mut pixels = vec![0u8; 32 * 32 * 4];
        for y in 2..30 { for x in 2..30 {
            let i = (y * 32 + x) * 4;
            let line = (x == 9 && (10..24).contains(&y)) || (x == 16 && (6..27).contains(&y)) || (x == 23 && (12..22).contains(&y));
            pixels[i..i + 4].copy_from_slice(if line { &[255, 255, 255, 255] } else { &[40, 108, 178, 255] });
        }}
        let tray = tray_icon::Icon::from_rgba(pixels, 32, 32).ok().and_then(|icon| TrayIconBuilder::new()
            .with_tooltip("Easy EQ — headphone EQ").with_icon(icon).with_menu(Box::new(menu))
            .with_menu_on_left_click(false).build().ok());
        crate::log(&format!("Native editor initialized. Editor visible: {}", !hidden));
        let notice = if tray.is_none() { "Tray icon unavailable; Close will keep the editor visible.".into() } else { String::new() };
        Self { native_revision: workspace.revision, base: workspace.clone(), workspace, controller, instance,
            sequence: 0, ear: 2, linked: false, selected: None, history: History::default(),
            settings_open: false, outputs_open: false, show_unavailable: false, rename: None, delete: None,
            quitting: false, tray_events: receiver, tray, notice, close_handled: false, screenshot, screenshot_at: Instant::now(), hidden,
            status_snapshot, status_sequence: 0, status_notice: String::new(), status_updated: Instant::now(),
            visualiser: Visualiser::default(),
            #[cfg(debug_assertions)] fixture_original: None,
        }
    }
    fn select_profile(&mut self, id: String) {
        self.workspace.active_id = id.clone(); self.selected = None; self.history.select(&id);
        if self.workspace.settings.profile_mode == ProfileMode::Global { self.workspace.settings.global_profile_id = Some(id); }
        else if let Some(output) = &self.workspace.settings.last_output_id { self.workspace.settings.device_profiles.insert(output.clone(), id); }
    }
    fn submit(&mut self) {
        if let Err(error) = self.workspace.validate() { self.notice = error.to_string(); return; }
        let sequence = self.sequence.saturating_add(1);
        if self.controller.sender.send(Command::Edit { client: self.workspace.clone(), base: self.base.clone(), sequence }).is_ok() {
            self.sequence = sequence;
            // Ordered edits use the last submitted base, including unacknowledged drag reversals.
            self.base = self.workspace.clone();
        } else { self.notice = "Audio host stopped. Reopen Easy EQ to reconnect.".into(); }
    }
    fn action(&mut self, action: u8, ctx: &egui::Context) {
        match action {
            0 => { self.hidden = false; ctx.send_viewport_cmd(ViewportCommand::Visible(true)); ctx.send_viewport_cmd(ViewportCommand::Focus); crate::log("Native editor reopened."); }
            1 | 2 => { let _ = self.controller.sender.send(Command::Enabled(action == 2)); }
            3 => { crate::supervisor::request_stop(); self.visualiser.stop(); self.quitting = true; self.controller.stop(); ctx.send_viewport_cmd(ViewportCommand::Close); crate::log("Native Quit: audio lease revoked."); }
            4 => { if self.tray.is_some() { self.hidden = true; ctx.send_viewport_cmd(ViewportCommand::Visible(false)); crate::log("Editor close intercepted; native host remains active. Editor visible: false"); } }
            #[cfg(debug_assertions)]
            5 => { if self.fixture_original.is_none() {
                self.fixture_original = Some(self.workspace.clone());
                let index = self.workspace.profiles.iter().position(|p| p.id == self.workspace.active_id).unwrap();
                let mut p = Profile::flat(self.workspace.active_id.clone(), self.workspace.profiles[index].name.clone());
                p.gain_db = -6.0; p.pan = 0.25;
                p.common.push(Band { id: "shared-audit".into(), kind: FilterKind::Peak, frequency: 1000.0, gain: -3.0, q: 1.4, enabled: true });
                p.ears.left.push(Band { id: "native-audit".into(), kind: FilterKind::Peak, frequency: 1000.0, gain: -6.0, q: 1.4, enabled: true });
                self.workspace.profiles[index] = p; self.workspace.enabled = true; self.submit();
                crate::log("Native controls fixture applied through the editor submission path.");
            }}
            #[cfg(debug_assertions)]
            6 => { if let Some(original) = self.fixture_original.take() { self.workspace = original; self.submit(); crate::log("Native controls fixture restored."); } }
            _ => {}
        }
        ctx.request_repaint();
    }
    fn profile_row(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let entries: Vec<_> = self.workspace.profiles.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
        let current = self.workspace.profiles.iter().find(|p| p.id == self.workspace.active_id).unwrap().name.clone();
        let mut active = self.workspace.active_id.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Profile").strong());
            egui::ComboBox::from_id_salt("profiles").width(220.0).selected_text(&current).show_ui(ui, |ui| {
                for (id, name) in entries { ui.selectable_value(&mut active, id, name); }
            });
            if active != self.workspace.active_id { self.select_profile(active); changed = true; }
            if ui.button("Rename").clicked() {
                let p = self.workspace.profiles.iter().find(|p| p.id == self.workspace.active_id).unwrap();
                self.rename = Some((p.id.clone(), p.name.clone()));
            }
            let room = self.workspace.profiles.len() < 100;
            if ui.add_enabled(room, egui::Button::new("New")).clicked() {
                let p = controller::flat(uid(), "New profile".into()); let id = p.id.clone();
                self.workspace.profiles.push(p); self.select_profile(id); changed = true;
            }
            if ui.add_enabled(room, egui::Button::new("Copy")).clicked() {
                let mut p = self.workspace.profiles.iter().find(|p| p.id == self.workspace.active_id).unwrap().clone();
                p.id = uid(); p.name = format!("{} copy", p.name.chars().take(75).collect::<String>());
                let id = p.id.clone(); self.workspace.profiles.push(p); self.select_profile(id); changed = true;
            }
            if ui.add_enabled(self.workspace.profiles.len() > 1, egui::Button::new("Delete")).clicked() { self.delete = Some(self.workspace.active_id.clone()); }
            ui.separator();
            if ui.add_enabled(room, egui::Button::new("Import…")).clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("Easy EQ profile", &["json"]).pick_file() {
                    let result = (|| -> Result<Profile, String> {
                        let mut raw = String::new();
                        std::fs::File::open(path).map_err(|e| e.to_string())?.take(1024 * 1024 + 1).read_to_string(&mut raw).map_err(|e| e.to_string())?;
                        easy_eq_core::parse_profile(&raw).map_err(|e| e.to_string())
                    })();
                    match result {
                        Ok(mut p) => { p.id = uid(); p.auto_headroom = false; p.pan_mode = PanMode::Balance;
                            let id = p.id.clone(); self.workspace.profiles.push(p); self.select_profile(id); changed = true; self.notice = "Profile imported.".into(); }
                        Err(error) => self.notice = error,
                    }
                }
            }
            if ui.button("Export…").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("Easy EQ profile", &["json"]).set_file_name("easy-eq-profile.json").save_file() {
                    let p = self.workspace.profiles.iter().find(|p| p.id == self.workspace.active_id).unwrap();
                    match serde_json::to_vec_pretty(p).map_err(|e| e.to_string()).and_then(|raw| std::fs::write(path, raw).map_err(|e| e.to_string())) {
                        Ok(()) => self.notice = "Profile exported.".into(), Err(error) => self.notice = error,
                    }
                }
            }
        });
        changed
    }
    fn band_toolbar(&mut self, ui: &mut egui::Ui, p: &mut Profile) {
        ui.horizontal_wrapped(|ui| {
            let linked = self.linked && self.ear < 2;
            let room = editor::bands(p, self.ear).len() < editor::capacity(p, self.ear, linked);
            ui.add_enabled_ui(room, |ui| { ui.menu_button("+ Add band", |ui| {
                for kind in [FilterKind::Peak, FilterKind::LowShelf, FilterKind::HighShelf] {
                    if ui.button(editor::shape_name(kind)).clicked() {
                        let band = Band { id: uid(), kind, frequency: match kind { FilterKind::Peak => 1000.0, FilterKind::LowShelf => 100.0, FilterKind::HighShelf => 8000.0 }, gain: 0.0, q: 0.707, enabled: true };
                        self.selected = Some(band.id.clone()); editor::add_band(p, self.ear, self.linked, band); ui.close();
                    }
                }
            }); });
            let index = editor::bands(p, self.ear).iter().position(|b| Some(&b.id) == self.selected.as_ref());
            if ui.add_enabled(index.is_some(), egui::Button::new("Remove")).clicked() {
                editor::remove_band(p, self.ear, self.linked, index.unwrap()); self.selected = None;
            }
            if let Some(index) = index.filter(|i| *i < editor::bands(p, self.ear).len() && self.selected.is_some()) {
                let band = &mut editor::bands_mut(p, self.ear)[index]; let old = band.kind;
                egui::ComboBox::from_id_salt("shape").width(94.0).selected_text(editor::shape_name(band.kind)).show_ui(ui, |ui| {
                    for kind in [FilterKind::Peak, FilterKind::LowShelf, FilterKind::HighShelf] { ui.selectable_value(&mut band.kind, kind, editor::shape_name(kind)); }
                });
                if old != band.kind && self.linked { editor::mirror_band(p, self.ear, index); }
            } else { ui.label(RichText::new("Select a band to change its shape").small().color(MUTED)); }
            ui.separator();
            if ui.button("Reset gains").on_hover_text("Reset the current stage's bands to 0 dB. Undo is available.").clicked() { editor::reset_ear(p, self.ear, self.linked); }
            if self.ear < 2 && ui.button(if self.ear == 0 { "Copy L to R" } else { "Copy R to L" }).clicked() { editor::copy_ear(p, self.ear); }
        });
    }
    fn bands(&mut self, ui: &mut egui::Ui, p: &mut Profile) {
        let color = match self.ear { 0 => graph::LEFT, 1 => graph::RIGHT, _ => graph::COMMON };
        let mut patches = Vec::new();
        if editor::bands(p, self.ear).is_empty() {
            egui::Frame::new().inner_margin(20.0).show(ui, |ui| {
                ui.label(if self.ear == 2 { "Shared EQ is flat. These filters apply to both ears before their individual correction." } else { "This ear has no extra correction. Shared EQ still applies." });
                let room = editor::capacity(p, self.ear, self.linked && self.ear < 2);
                if ui.add_enabled(room > 0, egui::Button::new("Start with flat bands")).clicked() {
                    *editor::bands_mut(p, self.ear) = controller::flat_bands().into_iter().take(room.min(13)).collect();
                    if self.linked && self.ear < 2 { editor::copy_ear(p, self.ear); }
                }
            }); return;
        }
        let width = ((ui.available_width() - 10.0) / editor::bands(p, self.ear).len().max(6) as f32 - 14.0).clamp(59.0, 108.0);
        egui::ScrollArea::horizontal().id_salt(("bands", self.ear)).auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for (index, band) in editor::bands_mut(p, self.ear).iter_mut().enumerate() {
                    let selected = self.selected.as_ref() == Some(&band.id);
                    let fill = if selected { Color32::from_rgb(222, 232, 243) } else { Color32::from_rgb(248, 249, 252) };
                    let mut edited = false;
                    ui.push_id((self.ear, &band.id), |ui| {
                        egui::Frame::new().fill(fill).corner_radius(4.0).inner_margin(4.0).show(ui, |ui| {
                            ui.set_width(width);
                            ui.vertical_centered(|ui| {
                                if ui.selectable_label(selected, RichText::new(format!("{}", index + 1)).color(color).strong()).clicked() { self.selected = Some(band.id.clone()); }
                                let old_kind = band.kind;
                                egui::ComboBox::from_id_salt("kind").width(width - 8.0).selected_text(editor::shape_name(band.kind)).show_ui(ui, |ui| {
                                    for kind in [FilterKind::Peak, FilterKind::LowShelf, FilterKind::HighShelf] { ui.selectable_value(&mut band.kind, kind, editor::shape_name(kind)); }
                                });
                                edited |= old_kind != band.kind;
                                edited |= ui.add(egui::DragValue::new(&mut band.frequency).range(10.0..=20000.0).speed(1.0).max_decimals(0).suffix(" Hz")).on_hover_text("Frequency. Drag the value or double-click to type.").changed();
                                edited |= ui.add_sized([24.0, 84.0], egui::Slider::new(&mut band.gain, -24.0..=24.0).vertical().show_value(false).clamping(egui::SliderClamping::Edits).step_by(0.1)).changed();
                                edited |= ui.add(egui::DragValue::new(&mut band.gain).range(-24.0..=24.0).speed(0.1).fixed_decimals(1).suffix(" dB")).on_hover_text("Gain in dB").changed();
                                edited |= ui.add(egui::DragValue::new(&mut band.q).range(0.1..=30.0).speed(0.05).fixed_decimals(2).prefix("Q ")).on_hover_text("Q: higher values affect a narrower frequency range.").changed();
                                edited |= ui.checkbox(&mut band.enabled, "On").changed();
                            });
                        });
                    });
                    if edited { patches.push(index); }
                }
            });
        });
        if self.linked && self.ear < 2 { for index in patches { editor::mirror_band(p, self.ear, index); } }
    }
    fn settings(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        egui::Window::new("Settings").open(&mut self.settings_open).collapsible(false).resizable(false).default_width(360.0).show(ctx, |ui| {
            changed |= ui.checkbox(&mut self.workspace.settings.start_at_login, "Start in tray at Windows sign-in").changed();
            ui.label(RichText::new("Restores your last EQ on/off state.").small().color(MUTED));
            ui.separator(); ui.label(RichText::new("Profiles and outputs").strong());
            let mut mode = self.workspace.settings.profile_mode;
            ui.radio_value(&mut mode, ProfileMode::PerDevice, "Remember a profile for each output");
            ui.radio_value(&mut mode, ProfileMode::Global, "Use one profile for every output");
            if mode != self.workspace.settings.profile_mode {
                self.workspace.settings.profile_mode = mode;
                if mode == ProfileMode::Global { self.workspace.settings.global_profile_id = Some(self.workspace.active_id.clone()); }
                changed = true;
            }
            ui.separator(); ui.label("Follows the Windows default output. New outputs start flat.");
            ui.label(RichText::new("Close / Done keeps EQ running. Quit from the tray stops it.").small().color(MUTED));
        }); changed
    }
    fn outputs(&mut self, ctx: &egui::Context, snapshot: &controller::Snapshot) -> bool {
        let mut changed = false;
        egui::Window::new("Audio outputs").open(&mut self.outputs_open).collapsible(false).default_width(640.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Refresh").clicked() { let _ = self.controller.sender.send(Command::Refresh); }
                ui.checkbox(&mut self.show_unavailable, "Show disconnected outputs");
                if let Some(report) = &snapshot.outputs { ui.small(format!("{} detected", report.outputs.len())); }
            });
            ui.label("The editor follows your Windows default output.");
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                if let Some(report) = &snapshot.outputs { for output in &report.outputs {
                    if !self.show_unavailable && output.state != "available" { continue; }
                    ui.separator();
                    ui.label(RichText::new(format!("{}{}", output.name, if output.is_default { "  · default" } else { "" })).strong());
                    let status = snapshot.audio.iter().find(|a| a.output_id == output.id);
                    ui.label(status.map(|a| a.reason.as_str()).unwrap_or(&output.reason));
                    if self.workspace.settings.profile_mode == ProfileMode::PerDevice {
                        let mut id = self.workspace.settings.device_profiles.get(&output.id).cloned().unwrap_or_default(); let old = id.clone();
                        let selected = self.workspace.profiles.iter().find(|p| p.id == id).map(|p| p.name.as_str()).unwrap_or("No profile assigned");
                        egui::ComboBox::from_id_salt(&output.id).selected_text(selected).width(220.0).show_ui(ui, |ui| {
                            for p in &self.workspace.profiles { ui.selectable_value(&mut id, p.id.clone(), &p.name); }
                        });
                        if old != id {
                            self.workspace.settings.device_profiles.insert(output.id.clone(), id.clone());
                            if output.is_default { self.workspace.active_id = id; } changed = true;
                        }
                    }
                }} else { ui.label("Detecting Windows audio outputs…"); }
            });
        }); changed
    }
    fn profile_dialogs(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        if let Some((id, name)) = self.rename.as_mut() {
            let mut open = true; let mut save = false;
            egui::Window::new("Rename profile").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                let response = ui.add(egui::TextEdit::singleline(name).char_limit(80).desired_width(270.0));
                let valid = !name.trim().is_empty();
                save = ui.add_enabled(valid, egui::Button::new("Save")).clicked()
                    || (valid && response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
            });
            if save { if let Some(p) = self.workspace.profiles.iter_mut().find(|p| &p.id == id) { p.name = name.trim().into(); changed = true; } }
            if save || !open { self.rename = None; }
        }
        if let Some(id) = self.delete.clone() {
            let mut open = true; let mut remove = false; let mut cancel = false;
            let name = self.workspace.profiles.iter().find(|p| p.id == id).map(|p| p.name.as_str()).unwrap_or("this profile");
            egui::Window::new("Delete profile?").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(format!("Delete “{name}” and its output assignments?"));
                ui.horizontal(|ui| { cancel = ui.button("Cancel").clicked(); remove = ui.button("Delete profile").clicked(); });
            });
            if remove && self.workspace.profiles.len() > 1 {
                self.workspace.profiles.retain(|p| p.id != id);
                self.workspace.settings.device_profiles.retain(|_, value| value != &id);
                if self.workspace.settings.global_profile_id.as_ref() == Some(&id) { self.workspace.settings.global_profile_id = None; }
                self.select_profile(self.workspace.profiles[0].id.clone()); changed = true;
            }
            if !open || remove || cancel { self.delete = None; }
        }
        changed
    }
    fn status_line(&self, ui: &mut egui::Ui) {
        let snapshot = &self.status_snapshot;
        let status = snapshot.audio.iter().find(|a| Some(&a.output_id) == snapshot.workspace.settings.last_output_id.as_ref());
        let (label, color) = match status.map(|s| s.state.as_str()) {
            Some("processing") => ("EQ active", Color32::from_rgb(27, 126, 83)),
            Some("partial") => ("EQ active · front L/R", Color32::from_rgb(163, 99, 20)),
            Some("bypassed") => ("Bypassed", MUTED),
            Some("waiting") => ("Waiting for playback", MUTED),
            Some("connecting") => ("Applying EQ…", graph::LEFT),
            Some("error") => ("Audio error", Color32::from_rgb(166, 47, 42)),
            Some("unavailable") => ("Output unavailable", MUTED),
            Some("needsSetup") => ("Audio setup needed", Color32::from_rgb(163, 99, 20)),
            _ => ("Connecting to audio…", MUTED),
        };
        ui.horizontal_wrapped(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 3.0, color);
            ui.colored_label(color, RichText::new(label).strong());
            let output = snapshot.outputs.as_ref().and_then(|r| r.outputs.iter().find(|o| o.is_default));
            ui.label(RichText::new(output.map(|o| o.name.as_str()).unwrap_or("Detecting default output")).small().color(MUTED));
            if let Some(rate) = status.and_then(|s| s.sample_rate) { ui.small(format!("{} Hz", rate as u32)); }
        });
        if let Some(error) = &snapshot.error { ui.colored_label(Color32::from_rgb(166, 47, 42), error); }
        if let Some(status) = status { if matches!(status.state.as_str(), "partial" | "needsSetup" | "error" | "unavailable") {
            ui.label(RichText::new(&status.reason).small().color(MUTED));
        }}
        ui.horizontal_wrapped(|ui| {
            let saved = if snapshot.acknowledged < self.status_sequence { "Applying changes…" }
                else if snapshot.saved_revision < snapshot.workspace.revision { "Saving…" } else { "Saved automatically" };
            ui.small(saved);
            if !self.status_notice.is_empty() { ui.separator(); ui.small(&self.status_notice); }
        });
    }
}
impl eframe::App for Desktop {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let actions: Vec<_> = self.tray_events.try_iter().chain(self.instance.drain().into_iter().map(|i| i as u8)).collect();
        for action in actions { self.action(action, ctx); }
        // eframe shows the root after its first paint regardless of the builder's
        // initial visibility. Keep native tray intent authoritative on later ticks.
        if self.hidden && self.tray.is_some() { ctx.send_viewport_cmd(ViewportCommand::Visible(false)); crate::instance::keep_hidden(frame); }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if !self.close_handled { self.action(4, ctx); self.close_handled = true; }
        } else { self.close_handled = false; }
        let snapshot = self.controller.snapshot();
        self.visualiser.poll(snapshot.workspace.settings.last_output_id.as_deref(), !self.hidden && !self.quitting);
        if snapshot.acknowledged >= self.sequence && snapshot.workspace.revision != self.native_revision {
            self.native_revision = snapshot.workspace.revision;
            self.workspace = snapshot.workspace.clone(); self.base = snapshot.workspace;
        }
        ctx.input(|input| { for event in &input.events {
            if let egui::Event::Screenshot { image, .. } = event {
                if let Some(path) = self.screenshot.take() {
                    let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(error) = image::save_buffer(&path, &rgba, image.width() as u32, image.height() as u32, image::ColorType::Rgba8) {
                        crate::log(&format!("Screenshot failed: {error}"));
                    } else { crate::log(&format!("Screenshot saved: {}", path.display())); }
                }
            }
        }});
        if self.screenshot.is_some() && self.screenshot_at.elapsed() > Duration::from_secs(1) {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        if !self.hidden { ctx.request_repaint_after(Duration::from_millis(33)); }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone(); let snapshot = self.controller.snapshot(); let mut changed = false;
        // Only the footer is sampled. Audio, editor controls and persistence stay immediate.
        if self.status_updated.elapsed() >= STATUS_INTERVAL {
            self.status_snapshot = snapshot.clone();
            self.status_sequence = self.sequence;
            self.status_notice = self.notice.clone();
            self.status_updated = Instant::now();
        }
        egui::Frame::central_panel(&ui.style()).inner_margin(12.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Easy EQ").size(21.0).strong()); ui.separator();
                    changed |= ui.checkbox(&mut self.workspace.enabled, RichText::new("EQ on").strong()).changed();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Done").on_hover_text("Hide the editor. EQ keeps running in the tray.").clicked() { self.action(4, &ctx); }
                        if ui.button("Settings").clicked() { self.settings_open = true; }
                        if ui.button("Audio outputs").clicked() { self.outputs_open = true; }
                    });
                });
                changed |= self.profile_row(ui); ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let old_ear = self.ear; ui.label(RichText::new("Edit").strong());
                    ui.selectable_value(&mut self.ear, 2, RichText::new("Both ears").color(graph::COMMON));
                    ui.selectable_value(&mut self.ear, 0, RichText::new("Left ear").color(graph::LEFT));
                    ui.selectable_value(&mut self.ear, 1, RichText::new("Right ear").color(graph::RIGHT));
                    if old_ear != self.ear { self.selected = None; self.history.end_gesture(); }
                    ui.add_enabled(self.ear < 2, egui::Checkbox::new(&mut self.linked, "Link ear edits")).on_hover_text("Copy individual band edits to the other ear. Both ears is the separate shared first stage.");
                    let mut overlay = self.workspace.view == "overlay";
                    if ui.checkbox(&mut overlay, "Show both").changed() { self.workspace.view = if overlay { "overlay" } else { "focus" }.into(); changed = true; }
                    ui.separator(); ui.label(RichText::new("Shared").color(graph::COMMON).strong()); ui.label(RichText::new("L").color(graph::LEFT).strong()); ui.label(RichText::new("R").color(graph::RIGHT).strong());
                    ui.label(RichText::new("Filter response").small().color(MUTED)).on_hover_text("Before output gain and balance.");
                });
                let index = self.workspace.profiles.iter().position(|p| p.id == self.workspace.active_id).unwrap();
                let mut p = self.workspace.profiles[index].clone(); self.history.select(&p.id);
                let status = snapshot.audio.iter().find(|a| Some(&a.output_id) == snapshot.workspace.settings.last_output_id.as_ref());
                let rate = status.and_then(|a| a.sample_rate).unwrap_or(48000.0); let before = p.clone();
                graph::show(ui, &mut p, self.ear, self.workspace.view == "overlay", &mut self.selected, self.linked, rate);
                self.band_toolbar(ui, &mut p); self.bands(ui, &mut p); ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Output gain").strong());
                    ui.add(egui::Slider::new(&mut p.gain_db, -60.0..=0.0).show_value(false).clamping(egui::SliderClamping::Edits).step_by(0.1));
                    ui.add(egui::DragValue::new(&mut p.gain_db).range(-60.0..=0.0).speed(0.1).fixed_decimals(1).suffix(" dB"));
                    ui.separator(); ui.label(RichText::new("Balance").strong()); ui.small("L");
                    ui.add(egui::Slider::new(&mut p.pan, -1.0..=1.0).show_value(false).clamping(egui::SliderClamping::Edits).step_by(0.01)); ui.small("R");
                    let mut balance_percent = p.pan * 100.0;
                    if ui.add(egui::DragValue::new(&mut balance_percent).range(-100.0..=100.0).speed(1.0).max_decimals(2).suffix(" %"))
                        .on_hover_text("Exact balance: negative = left, 0 = center, positive = right. Double-click to type a value.").changed() {
                        p.pan = balance_percent / 100.0;
                    }
                    ui.small(if p.pan == 0.0 { "Center" } else if p.pan < 0.0 { "Left" } else { "Right" });
                    if ui.small_button("Center").clicked() { p.pan = 0.0; }
                });
                self.history.record(&before, &p, ctx.input(|i| i.pointer.any_down()));
                let mut undo = false; let mut redo = false;
                ui.horizontal_wrapped(|ui| {
                    undo = ui.add_enabled(self.history.can_undo(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked();
                    redo = ui.add_enabled(self.history.can_redo(), egui::Button::new("Redo")).on_hover_text("Ctrl+Y / Ctrl+Shift+Z").clicked();
                    ui.separator(); ui.label(RichText::new("Drag nodes · scroll for Q · double-click to reset").small().color(MUTED));
                });
                if !ctx.egui_wants_keyboard_input() {
                    undo |= ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z));
                    redo |= ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Y) || i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z));
                    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) { self.workspace.enabled = !self.workspace.enabled; changed = true; }
                }
                if undo { self.history.undo(&mut p); } if redo { self.history.redo(&mut p); }
                if self.selected.as_ref().is_some_and(|id| !editor::bands(&p, self.ear).iter().any(|b| &b.id == id)) { self.selected = None; }
                changed |= p != self.workspace.profiles[index]; self.workspace.profiles[index] = p;
                ui.separator(); self.visualiser.show(ui, self.workspace.enabled);
                ui.separator(); self.status_line(ui);
            });
        });
        changed |= self.settings(&ctx); changed |= self.outputs(&ctx, &snapshot); changed |= self.profile_dialogs(&ctx);
        if changed { self.submit(); }
    }
}
