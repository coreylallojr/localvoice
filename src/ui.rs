//! The window. All behaviour lives in `Controller`; this file only draws it.

use crate::app::Controller;
use crate::engine::{self, Mode};
use crate::hotkeys;
use crate::voices::{self, Voice};
use eframe::egui::{
    self, Align, Button, Color32, ComboBox, CornerRadius, Frame, Layout, Margin, ProgressBar,
    RichText, Slider, Stroke, Vec2,
};
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

const ACCENT: Color32 = Color32::from_rgb(90, 210, 170);
const LIVE: Color32 = Color32::from_rgb(230, 70, 70);
const WARN: Color32 = Color32::from_rgb(240, 170, 60);
const EDITOR_FILL: Color32 = Color32::from_gray(32);
const MUTED_TEXT: Color32 = Color32::from_rgb(150, 156, 168);
/// Editor panel: labels (~80) + slider (150) + value box (~70) + spacing.
const EDITOR_CONTENT_WIDTH: f32 = 328.0;
const CABLE_URL: &str = "https://vb-audio.com/Cable/";

pub struct Ui {
    controller: Arc<Controller>,
    draft: Voice,
    draft_source: String,
    editor_message: Option<(String, bool)>,
    confirm_headphones: bool,
    confirm_delete: bool,
}

impl Ui {
    pub fn new(controller: Arc<Controller>) -> Self {
        let draft = controller.session().voice().clone();
        Self {
            draft_source: draft.id.clone(),
            draft,
            controller,
            editor_message: None,
            confirm_headphones: false,
            confirm_delete: false,
        }
    }

    pub fn style(ctx: &egui::Context) {
        ctx.set_visuals(egui::Visuals::dark());
        ctx.global_style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(10.0, 8.0);
            style.spacing.button_padding = Vec2::new(12.0, 6.0);
            style.spacing.slider_width = 220.0;
            style.visuals.selection.bg_fill = ACCENT.linear_multiply(0.55);
            style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
            style.visuals.slider_trailing_fill = true;
        });
    }

    fn sync_draft(&mut self) {
        let voice = self.controller.session().voice().clone();
        if voice.id != self.draft_source {
            self.draft_source = voice.id.clone();
            self.draft = voice;
            self.editor_message = None;
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.sync_draft();
        let mode = self.controller.mode();
        if mode != Mode::Stopped {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }

        egui::Panel::top("header")
            .frame(
                Frame::NONE
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin::symmetric(18, 12)),
            )
            .show(ui, |ui| self.header(ui, mode));
        // Explicit split: main column | fixed editor column, each clipped to its rect.
        let area = ui.available_rect_before_wrap();
        let split = area.max.x - (EDITOR_CONTENT_WIDTH + 32.0);
        let main_rect = egui::Rect::from_min_max(area.min, egui::pos2(split, area.max.y));
        let editor_rect = egui::Rect::from_min_max(egui::pos2(split, area.min.y), area.max);
        ui.painter()
            .rect_filled(main_rect, 0.0, ui.visuals().panel_fill);
        ui.painter().rect_filled(editor_rect, 0.0, EDITOR_FILL);
        ui.painter().vline(
            split,
            area.y_range(),
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(editor_rect.shrink(16.0)),
            |ui| {
                ui.set_clip_rect(editor_rect);
                egui::ScrollArea::vertical()
                    .id_salt("editor")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.editor(ui));
            },
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(main_rect.shrink2(Vec2::new(18.0, 8.0))), |ui| {
            ui.set_clip_rect(main_rect);
            {
                egui::ScrollArea::vertical()
                    .id_salt("main")
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        // Right gutter so the scrollbar never covers buttons or meters.
                        Frame::NONE.inner_margin(Margin { left: 0, right: 16, top: 0, bottom: 8 }).show(ui, |ui| {
                            self.devices(ui);
                            ui.add_space(6.0);
                            self.transport(ui, mode);
                            ui.add_space(6.0);
                            self.voice_grid(ui);
                            ui.add_space(6.0);
                            self.controls(ui, mode);
                            ui.add_space(10.0);
                            for line in [
                                "Hotkeys (work in games): Ctrl+Shift+Space live · Ctrl+Shift+M mute",
                                "Ctrl+Shift+B effect on/off · Ctrl+Shift+Left/Right switch voice",
                                "Closing the window keeps LocalVoice running in the system tray.",
                            ] {
                                ui.label(RichText::new(line).small().color(MUTED_TEXT));
                            }
                        });
                    });
            }
        });
        self.modals(ui.ctx());
    }

    fn header(&mut self, ui: &mut egui::Ui, mode: Mode) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("LocalVoice").size(22.0).strong());
            ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(MUTED_TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (text, color) = match mode {
                    Mode::Live if self.controller.shared.muted.load(Relaxed) => {
                        ("LIVE · MUTED", WARN)
                    }
                    Mode::Live if self.controller.shared.bypass.load(Relaxed) => {
                        ("LIVE · REAL VOICE", WARN)
                    }
                    Mode::Live => ("LIVE · VOICE ON", LIVE),
                    Mode::Test => ("HEADPHONE TEST", ACCENT),
                    Mode::Stopped => ("STOPPED", MUTED_TEXT),
                };
                pill(ui, text, color);
                if mode != Mode::Stopped {
                    let s = &self.controller.shared;
                    let latency = engine::latency_ms(&self.controller.session().voice().params);
                    let dropouts = s.dropouts.load(Relaxed);
                    let khz = s.status().rate as f32 / 1000.0;
                    ui.label(
                        RichText::new(format!(
                            "{khz:.1} kHz · ~{latency:.0} ms · DSP {:.0}% · {dropouts} dropouts",
                            s.load.get() * 100.0
                        ))
                        .color(if dropouts > 0 {
                            WARN
                        } else {
                            MUTED_TEXT
                        }),
                    );
                }
            });
        });
    }

    fn devices(&mut self, ui: &mut egui::Ui) {
        section(ui, "Devices", |ui| {
            let mut session = self.controller.session();
            let lists = session.devices.clone();
            let mut changed = false;
            egui::Grid::new("devices")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Microphone");
                    changed |= combo(
                        ui,
                        "mic",
                        &mut session.settings.input,
                        &lists.inputs,
                        "No microphone found",
                    );
                    ui.end_row();
                    ui.label("Virtual mic");
                    if lists.virtual_outputs.is_empty() {
                        ui.horizontal(|ui| {
                            ui.colored_label(WARN, "VB-CABLE not installed");
                            if ui.button("Set up…").clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(CABLE_URL));
                            }
                        });
                    } else {
                        changed |= combo(
                            ui,
                            "out",
                            &mut session.settings.output,
                            &lists.virtual_outputs,
                            "",
                        );
                    }
                    ui.end_row();
                    ui.label("Headphones");
                    changed |= combo(
                        ui,
                        "mon",
                        &mut session.settings.monitor,
                        &lists.monitors,
                        "No output found",
                    );
                    ui.end_row();
                });
            drop(session);
            ui.horizontal(|ui| {
                if ui.button("Refresh devices").clicked() {
                    self.controller.refresh_devices();
                }
                ui.label(
                    RichText::new(
                        "In Discord, games or OBS choose \"CABLE Output\" as your microphone.",
                    )
                    .small()
                    .color(MUTED_TEXT),
                );
            });
            if changed {
                self.controller.save_settings();
                if self.controller.mode() == Mode::Live {
                    self.controller.go_live(); // Reopen on the new devices.
                }
            }
        });
    }

    fn transport(&mut self, ui: &mut egui::Ui, mode: Mode) {
        let status = self.controller.shared.status();
        ui.columns(2, |columns| {
            let live = mode == Mode::Live;
            let (text, fill) = if live {
                ("STOP", LIVE)
            } else {
                ("GO LIVE", ACCENT.linear_multiply(0.8))
            };
            let go = Button::new(
                RichText::new(text)
                    .size(18.0)
                    .strong()
                    .color(Color32::BLACK),
            )
            .fill(fill)
            .corner_radius(CornerRadius::same(8));
            let size = Vec2::new(columns[0].available_width(), 52.0);
            if columns[0]
                .add_sized(size, go)
                .on_hover_text("Ctrl+Shift+Space")
                .clicked()
            {
                self.controller.toggle_live();
            }
            let monitoring = self.controller.shared.monitor_on.load(Relaxed);
            let text = match mode {
                Mode::Test => "STOP TEST",
                Mode::Live if monitoring => "HEADPHONES: ON",
                Mode::Live => "HEAR MYSELF",
                Mode::Stopped => "TEST IN HEADPHONES",
            };
            let active = mode == Mode::Test || (live && monitoring);
            let test = Button::new(RichText::new(text).size(16.0).strong())
                .selected(active)
                .corner_radius(CornerRadius::same(8));
            let size = Vec2::new(columns[1].available_width(), 52.0);
            if columns[1].add_sized(size, test).clicked() {
                if self.controller.session().settings.monitor_warning_seen
                    || mode == Mode::Test
                    || active
                {
                    self.controller.toggle_headphones();
                } else {
                    self.confirm_headphones = true;
                }
            }
        });
        let s = &self.controller.shared;
        let message = match mode {
            Mode::Live if s.muted.load(Relaxed) => {
                "LIVE but muted - apps hear silence (Ctrl+Shift+M to unmute).".to_string()
            }
            Mode::Live if s.bypass.load(Relaxed) => {
                "LIVE - apps hear your real voice (voice effect off).".to_string()
            }
            Mode::Live => format!(
                "LIVE - apps hear your {} voice.",
                self.controller.session().voice().name
            ),
            _ => status.message,
        };
        if !message.is_empty() {
            ui.colored_label(if status.is_error { LIVE } else { MUTED_TEXT }, message);
        }
        meter(ui, "Mic", s.input_level.get());
        meter(ui, "Voice", s.output_level.get());
    }

    fn controls(&mut self, ui: &mut egui::Ui, mode: Mode) {
        section(ui, "Controls", |ui| {
            let s = self.controller.shared.clone();
            let (mut gain, mut volume, mut ptt, key, mut hold, capturing) = {
                let session = self.controller.session();
                let st = &session.settings;
                (
                    st.input_gain_db,
                    st.monitor_volume,
                    st.ptt_enabled,
                    st.ptt_key.clone(),
                    st.ptt_mode == "hold",
                    session.capturing_key,
                )
            };
            egui::Grid::new("controls")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Mic gain");
                    let response = ui.add(
                        Slider::new(&mut gain, -12.0..=24.0)
                            .suffix(" dB")
                            .step_by(0.5),
                    );
                    if response.changed() {
                        self.controller.set_input_gain_db(gain);
                    }
                    if response.drag_stopped() || (response.changed() && !response.dragged()) {
                        self.controller.save_settings();
                    }
                    ui.end_row();
                    ui.label("Headphone volume");
                    let response = ui.add(
                        Slider::new(&mut volume, 0.0..=1.5)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                    );
                    if response.changed() {
                        self.controller.set_monitor_volume(volume);
                    }
                    if response.drag_stopped() || (response.changed() && !response.dragged()) {
                        self.controller.save_settings();
                    }
                    ui.end_row();
                });
            ui.horizontal_wrapped(|ui| {
                let muted = s.muted.load(Relaxed);
                let mut mute = Button::new(if muted { "Muted" } else { "Mute" });
                if muted {
                    mute = mute.fill(WARN.linear_multiply(0.45));
                }
                if ui.add(mute).on_hover_text("Ctrl+Shift+M - silences the virtual mic only").clicked() {
                    s.muted.store(!muted, Relaxed);
                }
                let bypass = s.bypass.load(Relaxed);
                let button = Button::new(if bypass { "Effect off" } else { "Effect on" }).selected(!bypass);
                if ui.add(button).on_hover_text("Ctrl+Shift+B - switch between the voice effect and your real voice").clicked() {
                    s.bypass.store(!bypass, Relaxed);
                }
                ui.separator();
                if ui.checkbox(&mut ptt, "Push-to-talk voice")
                    .on_hover_text("Your mic always transmits. The key switches the voice effect on (hold or toggle).")
                    .changed()
                {
                    self.controller.set_ptt_enabled(ptt);
                }
                let label = if capturing { "Press any key or mouse button…".to_string() } else { hotkeys::display_name(&key) };
                if ui.add_enabled(ptt, Button::new(label).selected(capturing)).on_hover_text("Click, then press the key you want").clicked() {
                    let mut session = self.controller.session();
                    session.capturing_key = !session.capturing_key;
                }
                ui.add_enabled_ui(ptt, |ui| {
                    let before = hold;
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut hold, true, "Hold");
                        ui.radio_value(&mut hold, false, "Toggle");
                    });
                    if hold != before {
                        self.controller.session().settings.ptt_mode = if hold { "hold" } else { "toggle" }.into();
                        self.controller.save_settings();
                    }
                });
            });
            if ptt && mode != Mode::Stopped {
                let effect_on = !s.bypass.load(Relaxed);
                let key = hotkeys::display_name(&key);
                let text = match (effect_on, hold) {
                    (true, _) => format!(
                        "Voice effect ON ({})",
                        self.controller.session().voice().name
                    ),
                    (false, true) => format!("Real voice - hold {key} for the voice effect"),
                    (false, false) => {
                        format!("Real voice - press {key} to switch the voice effect on")
                    }
                };
                ui.colored_label(if effect_on { ACCENT } else { MUTED_TEXT }, text);
            }
        });
    }

    fn voice_grid(&mut self, ui: &mut egui::Ui) {
        section(ui, "Voices", |ui| {
            let (voices, current) = {
                let session = self.controller.session();
                (session.voices.clone(), session.current)
            };
            // Even columns: as many 120 px+ cards per row as fit, at most 4.
            let gap = ui.spacing().item_spacing.x;
            let available = ui.available_width();
            let columns = (((available + gap) / (120.0 + gap)).floor() as usize).clamp(1, 4);
            let width = (available - gap * (columns - 1) as f32) / columns as f32;
            for row in voices.chunks(columns).enumerate() {
                let (row_index, row) = row;
                ui.horizontal(|ui| {
                    for (offset, voice) in row.iter().enumerate() {
                        let index = row_index * columns + offset;
                        let selected = index == current;
                        let text = RichText::new(&voice.name).strong().size(15.0);
                        let card = Button::new(text)
                            .selected(selected)
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(width, 46.0));
                        let hover = if voice.description.is_empty() {
                            voice.name.clone()
                        } else {
                            voice.description.clone()
                        };
                        if ui.add(card).on_hover_text(hover).clicked() {
                            self.controller.select_voice(index);
                        }
                    }
                });
            }
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.set_width(EDITOR_CONTENT_WIDTH);
        ui.spacing_mut().slider_width = 150.0;
        ui.label(RichText::new("Voice editor").size(18.0).strong());
        ui.label(
            RichText::new("Every change plays live. Save to keep it.")
                .small()
                .color(MUTED_TEXT),
        );
        ui.add_space(4.0);
        let builtin = self.draft.builtin;
        ui.add(
            egui::TextEdit::singleline(&mut self.draft.name)
                .hint_text("Name")
                .desired_width(f32::INFINITY),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.draft.description)
                .hint_text("Description")
                .desired_width(f32::INFINITY),
        );

        let before = self.draft.params;
        let p = &mut self.draft.params;
        group(ui, "Pitch & formant", |ui| {
            slider(
                ui,
                "Pitch",
                &mut p.pitch_semitones,
                -12.0..=12.0,
                " st",
                0.5,
                false,
            );
            slider(
                ui,
                "Formant",
                &mut p.formant_semitones,
                -12.0..=12.0,
                " st",
                0.5,
                false,
            );
        });
        ui.label(
            RichText::new("Formant 0 keeps it natural; match the pitch for a cartoon shift.")
                .small()
                .color(MUTED_TEXT),
        );
        group(ui, "Tone", |ui| {
            slider(
                ui,
                "Low cut",
                &mut p.highpass_hz,
                20.0..=1_000.0,
                " Hz",
                1.0,
                true,
            );
            slider(
                ui,
                "High cut",
                &mut p.lowpass_hz,
                1_000.0..=20_000.0,
                " Hz",
                10.0,
                true,
            );
            slider(
                ui,
                "Presence",
                &mut p.presence_db,
                -12.0..=12.0,
                " dB",
                0.5,
                false,
            );
        });
        group(ui, "Character", |ui| {
            slider(
                ui,
                "Grit",
                &mut p.distortion_db,
                0.0..=30.0,
                " dB",
                0.5,
                false,
            );
            slider(
                ui,
                "Bit depth",
                &mut p.bit_depth,
                2.0..=32.0,
                " bit",
                1.0,
                false,
            );
            slider(
                ui,
                "Ring mod",
                &mut p.ring_hz,
                0.0..=500.0,
                " Hz",
                1.0,
                false,
            );
            slider(ui, "Chorus", &mut p.chorus_mix, 0.0..=1.0, "%", 0.01, false);
        });
        group(ui, "Space", |ui| {
            slider(ui, "Reverb", &mut p.reverb_mix, 0.0..=1.0, "%", 0.01, false);
        });
        group(ui, "Cleanup", |ui| {
            ui.label("Denoise");
            ui.checkbox(&mut p.denoise, "RNNoise")
                .on_hover_text("Removes keyboard, fan and room noise. Adds ~10 ms.");
            ui.end_row();
            slider(
                ui,
                "Noise gate",
                &mut p.gate_db,
                -100.0..=0.0,
                " dB",
                1.0,
                false,
            );
        });
        group(ui, "Output", |ui| {
            slider(ui, "Gain", &mut p.gain_db, -24.0..=24.0, " dB", 0.5, false);
        });
        if self.draft.params.highpass_hz >= self.draft.params.lowpass_hz {
            self.draft.params.lowpass_hz = (self.draft.params.highpass_hz + 500.0).min(20_000.0);
        }
        if self.draft.params != before {
            self.controller.preview(self.draft.params);
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if !builtin && ui.button("Save").clicked() {
                self.finish(
                    self.controller.save_voice(self.draft.clone(), false),
                    "Saved.",
                );
            }
            let label = if builtin {
                "Save as my voice"
            } else {
                "Save as new"
            };
            if ui.button(label).clicked() {
                let mut voice = self.draft.clone();
                if builtin && voice.name == self.controller.session().voice().name {
                    voice.name = format!("{} (mine)", voice.name);
                }
                self.finish(
                    self.controller.save_voice(voice, true),
                    "Saved as a new voice.",
                );
            }
            if ui.button("Revert").clicked() {
                self.draft = self.controller.session().voice().clone();
                self.controller.revert_preview();
                self.editor_message = None;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Import pack…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("LocalVoice pack", &["voicepack", "zip"])
                    .pick_file()
            {
                let result = self
                    .controller
                    .import_voice(&path)
                    .map(|name| format!("Imported {name}."));
                self.editor_message = Some(match result {
                    Ok(text) => (text, false),
                    Err(error) => (error, true),
                });
            }
            if ui.button("Export pack…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_file_name(format!("{}.voicepack", self.draft.name))
                    .add_filter("LocalVoice pack", &["voicepack"])
                    .save_file()
            {
                let result = voices::export_pack(&self.draft, &path);
                self.finish(result, "Exported.");
            }
            if !builtin && ui.button(RichText::new("Delete").color(LIVE)).clicked() {
                self.confirm_delete = true;
            }
        });
        if let Some((text, error)) = &self.editor_message {
            ui.colored_label(if *error { LIVE } else { ACCENT }, text);
        }
    }

    fn finish(&mut self, result: Result<(), String>, success: &str) {
        let message = match result {
            Ok(()) => {
                // Saved/deleted voices change the selection; show what was stored.
                self.draft_source.clear();
                self.sync_draft();
                (success.to_string(), false)
            }
            Err(error) => (error, true),
        };
        self.editor_message = Some(message);
    }

    fn modals(&mut self, ctx: &egui::Context) {
        if self.confirm_headphones {
            egui::Modal::new(egui::Id::unique("headphones")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new("Wear headphones").size(18.0).strong());
                ui.label(
                    "Monitoring through speakers can cause loud feedback. Put headphones on first.",
                );
                ui.horizontal(|ui| {
                    if ui.button("I'm wearing headphones").clicked() {
                        self.confirm_headphones = false;
                        self.controller.session().settings.monitor_warning_seen = true;
                        self.controller.save_settings();
                        self.controller.toggle_headphones();
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_headphones = false;
                    }
                });
            });
        }
        if self.confirm_delete {
            egui::Modal::new(egui::Id::unique("delete")).show(ctx, |ui| {
                ui.label(format!(
                    "Delete \"{}\"? This cannot be undone.",
                    self.draft.name
                ));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Delete").color(LIVE)).clicked() {
                        self.confirm_delete = false;
                        let result = self.controller.delete_voice();
                        self.finish(result, "Deleted.");
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_delete = false;
                    }
                });
            });
        }
    }
}

fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::NONE
        .fill(color.linear_multiply(0.18))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| ui.label(RichText::new(text).strong().color(color)));
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(
        RichText::new(title.to_uppercase())
            .small()
            .strong()
            .color(MUTED_TEXT),
    );
    Frame::group(ui.style())
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).strong().color(ACCENT));
    egui::Grid::new(title)
        .num_columns(2)
        .min_col_width(78.0)
        .spacing([10.0, 6.0])
        .show(ui, add);
}

fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    step: f64,
    logarithmic: bool,
) {
    ui.label(label);
    let slider = Slider::new(value, range)
        .step_by(step)
        .logarithmic(logarithmic);
    if suffix == "%" {
        // 0..1 mix amounts read better as percentages.
        ui.add(
            slider
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                .custom_parser(|text| {
                    text.trim_end_matches('%')
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|v| v / 100.0)
                }),
        );
    } else if suffix == " bit" {
        // 32 bit means no bit crushing.
        ui.add(slider.custom_formatter(|v, _| {
            if v >= 31.5 {
                "Off".into()
            } else {
                format!("{v:.0} bit")
            }
        }));
    } else {
        ui.add(slider.suffix(suffix));
    }
    ui.end_row();
}

fn meter(ui: &mut egui::Ui, label: &str, level: f32) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [44.0, 16.0],
            egui::Label::new(RichText::new(label).small().color(MUTED_TEXT)),
        );
        let db = 20.0 * level.max(1e-5).log10();
        let fill = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let color = if db > -3.0 {
            LIVE
        } else if db > -12.0 {
            WARN
        } else {
            ACCENT
        };
        ui.add(ProgressBar::new(fill).desired_height(10.0).fill(color));
    });
}

fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[String], empty: &str) -> bool {
    let mut changed = false;
    let shown = if value.is_empty() {
        empty.to_string()
    } else {
        value.clone()
    };
    ComboBox::from_id_salt(id)
        .width(ui.available_width().min(420.0))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for option in options {
                changed |= ui.selectable_value(value, option.clone(), option).changed();
            }
        });
    changed
}
