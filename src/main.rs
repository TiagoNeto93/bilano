#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod icon;
mod single;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::epaint::{Mesh, Vertex};
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Shape, Stroke};

use audio::{Cmd, Shared};
use config::Config;

const BLUE: Color32 = Color32::from_rgb(74, 137, 255);
const GREEN: Color32 = Color32::from_rgb(52, 205, 130);
const STEP: f32 = 0.1;

fn main() -> eframe::Result<()> {
    // Single instance: a second launch surfaces the running window and exits.
    let _instance = match single::acquire() {
        Some(i) => i,
        None => return Ok(()),
    };

    // Shared state across the UI, engine, and tray/hotkey threads.
    let cfg = Arc::new(Mutex::new(Config::load()));
    let shared = Shared::new();
    let tx = audio::spawn(shared.clone());
    let quit_flag = Arc::new(AtomicBool::new(false));

    {
        let c = cfg.lock().unwrap();
        tx.send(Cmd::SetChat(c.chat_set())).ok();
        tx.send(Cmd::SetMix(c.mix)).ok();
    }

    tray::spawn(tx.clone(), cfg.clone(), shared.clone(), quit_flag.clone());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([360.0, 540.0])
            .with_min_inner_size([330.0, 460.0])
            .with_title("ChatMix")
            .with_icon(Arc::new(egui::IconData {
                rgba: icon::rgba(64),
                width: 64,
                height: 64,
            })),
        ..Default::default()
    };
    eframe::run_native(
        "ChatMix",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, tx, shared, cfg, quit_flag)))),
    )
}

struct App {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
    cfg: Arc<Mutex<Config>>,
    quit_flag: Arc<AtomicBool>,
    icon_tex: egui::TextureHandle,
    new_app: String,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        tx: Sender<Cmd>,
        shared: Arc<Shared>,
        cfg: Arc<Mutex<Config>>,
        quit_flag: Arc<AtomicBool>,
    ) -> Self {
        load_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        let icon_tex = cc.egui_ctx.load_texture(
            "app-icon",
            egui::ColorImage::from_rgba_unmultiplied([48, 48], &icon::rgba(48)),
            egui::TextureOptions::LINEAR,
        );
        App {
            tx,
            shared,
            cfg,
            quit_flag,
            icon_tex,
            new_app: String::new(),
        }
    }

    fn mix(&self) -> f32 {
        self.cfg.lock().map(|c| c.mix).unwrap_or(0.0)
    }

    fn set_mix(&self, mix: f32) {
        let mix = mix.clamp(-1.0, 1.0);
        if let Ok(mut c) = self.cfg.lock() {
            c.mix = mix;
            c.save();
        }
        self.tx.send(Cmd::SetMix(mix)).ok();
    }

    fn is_chat(&self, exe: &str) -> bool {
        self.cfg
            .lock()
            .map(|c| c.chat.iter().any(|x| x.eq_ignore_ascii_case(exe)))
            .unwrap_or(false)
    }

    fn set_chat(&self, exe: &str, on: bool) {
        let set = {
            let mut c = match self.cfg.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            let el = exe.to_lowercase();
            let has = c.chat.iter().any(|x| x.eq_ignore_ascii_case(&el));
            if on && !has {
                c.chat.push(el);
            } else if !on && has {
                c.chat.retain(|x| !x.eq_ignore_ascii_case(&el));
            } else {
                return;
            }
            c.save();
            c.chat_set()
        };
        self.tx.send(Cmd::SetChat(set)).ok();
    }

    fn autostart(&self) -> bool {
        self.cfg.lock().map(|c| c.autostart).unwrap_or(false)
    }

    fn set_autostart(&self, on: bool) {
        if let Ok(mut c) = self.cfg.lock() {
            c.autostart = on;
            c.save();
        }
        set_autostart_reg(on);
    }

    /// (exe, active, level) rows: live apps merged with configured chat apps.
    fn rows(&self) -> Vec<(String, bool, f32)> {
        let live = self.shared.apps.lock().map(|g| g.clone()).unwrap_or_default();
        let mut rows: Vec<(String, bool, f32)> =
            live.iter().map(|a| (a.exe.clone(), a.active, a.vol)).collect();
        if let Ok(c) = self.cfg.lock() {
            for exe in &c.chat {
                if !rows.iter().any(|(e, _, _)| e.eq_ignore_ascii_case(exe)) {
                    rows.push((exe.clone(), false, 0.0));
                }
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }
}

impl eframe::App for App {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.086, 0.090, 0.102, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // Close button hides to tray instead of quitting.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            tray::hide_window();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(16.0_f32))
            .show(ctx, |ui| {
                // ---- Header ----
                ui.horizontal(|ui| {
                    ui.image(egui::load::SizedTexture::new(
                        self.icon_tex.id(),
                        vec2(36.0, 36.0),
                    ));
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ChatMix").size(21.0).strong());
                        ui.label(
                            RichText::new("game ↔ voice balance")
                                .size(11.0)
                                .color(Color32::from_gray(140)),
                        );
                    });
                });

                ui.add_space(14.0);

                let mut mix = self.mix();

                // ---- Balance readout ----
                let pct = (mix * 100.0).round() as i32;
                let (txt, col) = if pct == 0 {
                    ("Balanced".to_string(), Color32::from_gray(200))
                } else if pct > 0 {
                    (format!("{}%  →  Game", pct), GREEN)
                } else {
                    (format!("Chat  ←  {}%", -pct), BLUE)
                };
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(txt).size(16.0).strong().color(col));
                });
                ui.add_space(6.0);

                // ---- Balance slider ----
                if balance_slider(ui, &mut mix).changed() {
                    self.set_mix(mix);
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Chat").size(12.0).strong().color(BLUE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("Game").size(12.0).strong().color(GREEN));
                    });
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let bw = (ui.available_width() - 12.0) / 3.0;
                    if ui.add_sized([bw, 26.0], egui::Button::new("◀ Chat")).clicked() {
                        self.set_mix(mix - STEP);
                    }
                    if ui.add_sized([bw, 26.0], egui::Button::new("Center")).clicked() {
                        self.set_mix(0.0);
                    }
                    if ui.add_sized([bw, 26.0], egui::Button::new("Game ▶")).clicked() {
                        self.set_mix(mix + STEP);
                    }
                });
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Ctrl+Alt+←  Chat   ·   Ctrl+Alt+→  Game   ·   Ctrl+Alt+↓  Center")
                            .size(10.0)
                            .color(Color32::from_gray(120)),
                    );
                });

                ui.add_space(12.0);
                ui.label(RichText::new("APPS").size(11.0).color(Color32::from_gray(130)).strong());
                ui.label(
                    RichText::new("tick the ones that are voice chat")
                        .size(11.0)
                        .color(Color32::from_gray(120)),
                );
                ui.add_space(4.0);

                let rows = self.rows();
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (exe, active, level) in &rows {
                            let chat = self.is_chat(exe);
                            if let Some(on) = app_row(ui, exe, *active, *level, chat) {
                                self.set_chat(exe, on);
                            }
                            ui.add_space(4.0);
                        }
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Add:").size(12.0).color(Color32::from_gray(150)));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.new_app)
                            .desired_width(140.0)
                            .hint_text("app.exe"),
                    );
                    let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("+ Chat").clicked() || submit {
                        let mut name = self.new_app.trim().to_lowercase();
                        if !name.is_empty() {
                            if !name.ends_with(".exe") {
                                name.push_str(".exe");
                            }
                            self.set_chat(&name, true);
                            self.new_app.clear();
                        }
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let mut autostart = self.autostart();
                    if ui.checkbox(&mut autostart, "Start with Windows").changed() {
                        self.set_autostart(autostart);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Quit").clicked() {
                            self.quit_flag.store(true, Ordering::SeqCst);
                        }
                        if ui.button("Hide").clicked() {
                            tray::hide_window();
                        }
                    });
                });
            });
    }
}

/// One app row: checkbox + status dot + name + tag chip + level bar.
/// Returns `Some(new_state)` when the checkbox is toggled.
fn app_row(ui: &mut egui::Ui, exe: &str, active: bool, level: f32, is_chat: bool) -> Option<bool> {
    let mut toggled = None;
    egui::Frame::none()
        .fill(Color32::from_gray(28))
        .rounding(7.0_f32)
        .inner_margin(egui::Margin::symmetric(9.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut chat = is_chat;
                if ui.checkbox(&mut chat, "").changed() {
                    toggled = Some(chat);
                }
                let dot_col = if active { GREEN } else { Color32::from_gray(80) };
                ui.label(RichText::new("●").size(9.0).color(dot_col));
                ui.label(RichText::new(exe).size(13.0));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(52.0, 7.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.5_f32, Color32::from_gray(45));
                    let fill =
                        Rect::from_min_size(r.min, vec2(r.width() * level.clamp(0.0, 1.0), r.height()));
                    let barcol = if is_chat { BLUE } else { GREEN };
                    ui.painter().rect_filled(fill, 3.5_f32, barcol);
                    ui.add_space(6.0);
                    let (bg, fg, t) = if is_chat {
                        (BLUE, Color32::WHITE, "CHAT")
                    } else {
                        (Color32::from_gray(55), Color32::from_gray(190), "GAME")
                    };
                    ui.label(RichText::new(t).size(10.0).strong().color(fg).background_color(bg));
                });
            });
        });
    toggled
}

/// Custom Chat↔Game slider with a gradient track and a knob.
fn balance_slider(ui: &mut egui::Ui, value: &mut f32) -> egui::Response {
    let w = ui.available_width();
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click_and_drag());

    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let mut v = (t * 2.0 - 1.0).clamp(-1.0, 1.0);
            if v.abs() < 0.04 {
                v = 0.0;
            }
            if v != *value {
                *value = v;
                resp.mark_changed();
            }
        }
    }

    let painter = ui.painter();
    let track_h = 12.0;
    let ty = rect.center().y;
    let track = Rect::from_min_max(
        pos2(rect.left() + 6.0, ty - track_h / 2.0),
        pos2(rect.right() - 6.0, ty + track_h / 2.0),
    );

    let mut mesh = Mesh::default();
    let uv = egui::epaint::WHITE_UV;
    mesh.vertices.push(Vertex { pos: track.left_top(), uv, color: BLUE });
    mesh.vertices.push(Vertex { pos: track.left_bottom(), uv, color: BLUE });
    mesh.vertices.push(Vertex { pos: track.right_top(), uv, color: GREEN });
    mesh.vertices.push(Vertex { pos: track.right_bottom(), uv, color: GREEN });
    mesh.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    painter.add(Shape::mesh(mesh));

    let cx = rect.center().x;
    painter.line_segment(
        [pos2(cx, ty - track_h / 2.0 - 4.0), pos2(cx, ty + track_h / 2.0 + 4.0)],
        Stroke::new(1.5_f32, Color32::from_white_alpha(70)),
    );

    let t = (*value * 0.5 + 0.5).clamp(0.0, 1.0);
    let kx = track.left() + t * track.width();
    let kc = pos2(kx, ty);
    let kr = 11.0;
    painter.circle_filled(pos2(kx, ty + 1.5), kr, Color32::from_black_alpha(70));
    painter.circle_filled(kc, kr, Color32::from_rgb(250, 250, 252));
    painter.circle_stroke(kc, kr, Stroke::new(1.0_f32, Color32::from_black_alpha(50)));
    let accent = if *value > 0.0 {
        GREEN
    } else if *value < 0.0 {
        BLUE
    } else {
        Color32::from_gray(150)
    };
    painter.circle_filled(kc, 3.5, accent);

    resp
}

/// Load Windows' Segoe UI (+ Segoe UI Symbol for arrows) as primary fonts, so
/// no glyph shows as a tofu box and the UI looks native. Zero bundled fonts.
fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        ("segoeui", r"C:\Windows\Fonts\segoeui.ttf"),
        ("seguisym", r"C:\Windows\Fonts\seguisym.ttf"),
    ];
    let mut loaded = Vec::new();
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_string(), egui::FontData::from_owned(bytes));
            loaded.push(name.to_string());
        }
    }
    if !loaded.is_empty() {
        let prop = fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default();
        for name in loaded.iter().rev() {
            prop.insert(0, name.clone());
        }
        ctx.set_fonts(fonts);
    }
}

fn setup_style(ctx: &egui::Context) {
    use egui::{FontFamily::Proportional, FontId, TextStyle};
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(22, 23, 26);
    style.visuals.window_fill = Color32::from_rgb(22, 23, 26);
    style.visuals.override_text_color = Some(Color32::from_gray(225));
    style.visuals.widgets.noninteractive.rounding = 7.0_f32.into();
    style.visuals.widgets.inactive.rounding = 7.0_f32.into();
    style.visuals.widgets.hovered.rounding = 7.0_f32.into();
    style.visuals.widgets.active.rounding = 7.0_f32.into();
    style.visuals.selection.bg_fill = BLUE;
    style.visuals.widgets.inactive.bg_fill = Color32::from_gray(40);
    style.visuals.widgets.hovered.bg_fill = Color32::from_gray(52);
    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(10.0, 4.0);
    style.text_styles = [
        (TextStyle::Heading, FontId::new(21.0, Proportional)),
        (TextStyle::Body, FontId::new(13.5, Proportional)),
        (TextStyle::Button, FontId::new(13.0, Proportional)),
        (TextStyle::Small, FontId::new(11.0, Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, egui::FontFamily::Monospace)),
    ]
    .into();
    ctx.set_style(style);
}

/// Toggle the HKCU Run entry via reg.exe (no extra crate/feature needed).
fn set_autostart_reg(on: bool) {
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => return,
    };
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut cmd = std::process::Command::new("reg");
    if on {
        cmd.args(["add", key, "/v", "ChatMix", "/t", "REG_SZ", "/d", &exe, "/f"]);
    } else {
        cmd.args(["delete", key, "/v", "ChatMix", "/f"]);
    }
    let _ = cmd.output();
}
