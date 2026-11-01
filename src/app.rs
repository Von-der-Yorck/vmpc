//! Application shell: header, tabs, the player view, playlist, shortcuts and
//! drag & drop routing.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{pos2, vec2, Align, Align2, Color32, FontId, Key, Layout, Margin, Rect, RichText, Sense, Ui, Vec2};

use crate::converter::Converter;
use crate::ffmpeg;
use crate::frames::Frames;
use crate::icons::{self, Icon};
use crate::jobs::Queue;
use crate::player::{LoadState, MaxRes, Player};
use crate::settings::{self, Settings};
use crate::theme::{self, pal};
use crate::util;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    Off,
    All,
    One,
}

const TABS: [(Icon, &str); 3] = [(Icon::Play, "Player"), (Icon::Convert, "Convert"), (Icon::Frames, "Frames")];

pub struct App {
    tab: usize,
    dark: bool,
    player: Player,
    playlist: Vec<String>,
    current: Option<usize>,
    show_playlist: bool,
    loop_mode: LoopMode,
    seek_drag: Option<f32>,
    url_input: String,
    converter: Converter,
    frames: Frames,
    queue: Queue,
    toast: Option<(String, Instant)>,
    last_pointer_move: Instant,
    show_settings: bool,
    level: f32,
    title: String,
    /// Last settings written to disk; compared each frame to persist changes.
    saved: Settings,
    settings_dirty: Option<Instant>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<String>) -> Self {
        // The UI font lacks arrows (→ ← ↑ ↓ ≈); the bundled monospace font has them.
        let mut fonts = egui::FontDefinitions::default();
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.push("Hack".to_owned());
        }
        cc.egui_ctx.set_fonts(fonts);
        let saved = settings::load();
        theme::apply(&cc.egui_ctx, saved.dark);
        let mut player = Player::default();
        player.volume = saved.volume.clamp(0.0, 1.0);
        player.muted = saved.muted;
        player.max_res = MaxRes::from_height(saved.max_height);
        player.hwaccel = saved.hwaccel;
        let mut app = Self {
            tab: 0,
            dark: saved.dark,
            player,
            playlist: Vec::new(),
            current: None,
            show_playlist: saved.show_playlist,
            loop_mode: match saved.loop_mode {
                1 => LoopMode::All,
                2 => LoopMode::One,
                _ => LoopMode::Off,
            },
            saved,
            seek_drag: None,
            url_input: String::new(),
            converter: Converter::default(),
            frames: Frames::default(),
            queue: Queue::new(cc.egui_ctx.clone()),
            toast: None,
            last_pointer_move: Instant::now(),
            show_settings: false,
            level: 0.0,
            title: String::new(),
            settings_dirty: None,
        };
        if !files.is_empty() {
            app.enqueue(files, true);
        }
        app
    }

    // ----- playlist ---------------------------------------------------------

    fn enqueue(&mut self, sources: Vec<String>, play_first: bool) {
        let first_new = self.playlist.len();
        self.playlist.extend(sources);
        if play_first && self.playlist.len() > first_new {
            self.play_index(first_new);
        }
        if self.playlist.len() > 1 {
            self.show_playlist = true;
        }
    }

    fn play_index(&mut self, i: usize) {
        if let Some(src) = self.playlist.get(i).cloned() {
            self.current = Some(i);
            self.seek_drag = None;
            self.player.open(src);
        }
    }

    fn next(&mut self) {
        match self.current {
            Some(i) if i + 1 < self.playlist.len() => self.play_index(i + 1),
            Some(_) if self.loop_mode == LoopMode::All && !self.playlist.is_empty() => self.play_index(0),
            _ => {}
        }
    }

    fn prev(&mut self) {
        if self.player.position() > 3.0 {
            self.player.seek(0.0);
            return;
        }
        match self.current {
            Some(i) if i > 0 => self.play_index(i - 1),
            _ => self.player.seek(0.0),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(files) = rfd::FileDialog::new().set_title("Open media").pick_files() {
            let files: Vec<String> = files.into_iter().map(|f| f.to_string_lossy().into_owned()).collect();
            self.enqueue(files, true);
        }
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn snapshot(&mut self) {
        let dir = util::default_output_dir("Pictures");
        match self.player.snapshot(dir) {
            Some(p) => self.toast(format!("Snapshot saved: {}", p.display())),
            None => self.toast("Nothing to snapshot"),
        }
    }

    fn toggle_fullscreen(&self, ctx: &egui::Context) {
        let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
    }

    // ----- input --------------------------------------------------------------

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if dropped.is_empty() {
            return;
        }
        match self.tab {
            0 => {
                let files: Vec<String> = dropped
                    .iter()
                    .flat_map(|p| {
                        if p.is_dir() {
                            let mut v: Vec<PathBuf> = std::fs::read_dir(p)
                                .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
                                .unwrap_or_default();
                            v.sort_by(|a, b| util::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
                            v
                        } else {
                            vec![p.clone()]
                        }
                    })
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                self.enqueue(files, true);
            }
            1 => {
                for p in dropped {
                    self.converter.add(p.to_string_lossy().into_owned(), ctx);
                }
            }
            _ => self.frames.add_paths(dropped, ctx),
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.pointer.any_pressed()) {
            self.last_pointer_move = Instant::now();
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        let (keys, mods) = ctx.input(|i| {
            let keys: Vec<Key> = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, .. } => Some(*key),
                    _ => None,
                })
                .collect();
            (keys, i.modifiers)
        });
        for key in keys {
            match key {
                Key::O if mods.command => self.open_dialog(),
                Key::Escape => ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false)),
                _ if self.tab != 0 => {}
                Key::Space | Key::K => self.player.toggle_pause(),
                Key::ArrowRight | Key::ArrowLeft => {
                    let step = if mods.command { 30.0 } else if mods.shift { 1.0 } else { 5.0 };
                    let dir = if key == Key::ArrowRight { 1.0 } else { -1.0 };
                    let t = self.player.position() + step * dir;
                    self.player.seek(t);
                }
                Key::ArrowUp => self.player.set_volume(self.player.volume + 0.05),
                Key::ArrowDown => self.player.set_volume(self.player.volume - 0.05),
                Key::M => self.player.set_muted(!self.player.muted),
                Key::F | Key::F11 => self.toggle_fullscreen(ctx),
                Key::N => self.next(),
                Key::P => self.prev(),
                Key::E => self.player.step_frame(),
                Key::S => self.snapshot(),
                Key::L if mods.command => self.show_playlist = !self.show_playlist,
                Key::L => self.cycle_loop(),
                Key::Plus | Key::Equals => self.player.set_speed(self.player.speed * 1.25),
                Key::Minus => self.player.set_speed(self.player.speed / 1.25),
                _ => {}
            }
        }
    }

    fn cycle_loop(&mut self) {
        self.loop_mode = match self.loop_mode {
            LoopMode::Off => LoopMode::All,
            LoopMode::All => LoopMode::One,
            LoopMode::One => LoopMode::Off,
        };
        let text = match self.loop_mode {
            LoopMode::Off => "Loop off",
            LoopMode::All => "Loop playlist",
            LoopMode::One => "Loop current item",
        };
        self.toast(text);
    }

    // ----- header ---------------------------------------------------------------

    fn header(&mut self, ctx: &egui::Context) {
        let p = pal();
        egui::TopBottomPanel::top("header")
            .exact_height(78.0)
            .frame(egui::Frame::none().fill(p.bg).inner_margin(Margin::symmetric(22.0, 12.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
                    theme::paint_raised(ui.painter(), rect.shrink(3.0), 12.0, 4.0);
                    icons::paint_logo(ui.painter(), rect);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("VMPC").size(22.0).strong().color(p.strong));
                        ui.add_space(-8.0);
                        ui.label(RichText::new("Video · Media · Player · Converter").size(11.5).color(p.muted));
                    });
                    ui.add_space(24.0);
                    let mut tab = self.tab;
                    theme::segmented(ui, &mut tab, &TABS);
                    self.tab = tab;
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::icon_button(ui, Icon::Gear, 40.0, self.show_settings, "Settings").clicked() {
                            self.show_settings = !self.show_settings;
                        }
                        ui.add_space(4.0);
                        let (icon, tip) = if self.dark { (Icon::Sun, "Light mode") } else { (Icon::Moon, "Dark mode") };
                        if theme::icon_button(ui, icon, 40.0, false, tip).clicked() {
                            self.dark = !self.dark;
                            theme::apply(ctx, self.dark);
                        }
                        if self.queue.is_busy() {
                            ui.add_space(8.0);
                            ui.add(egui::Spinner::new().color(p.accent));
                            ui.label(RichText::new("Converting…").color(p.muted));
                        }
                    });
                });
            });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let p = pal();
        let mut open = self.show_settings;
        egui::Window::new(RichText::new("Settings").strong().color(p.strong))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .anchor(Align2::RIGHT_TOP, vec2(-20.0, 86.0))
            .frame(egui::Frame::window(&ctx.style()).inner_margin(18.0))
            .show(ctx, |ui| {
                theme::section_title(ui, "Playback");
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Max video render size");
                    egui::ComboBox::from_id_salt("maxres").selected_text(self.player.max_res.label()).show_ui(ui, |ui| {
                        for r in MaxRes::ALL {
                            changed |= ui.selectable_value(&mut self.player.max_res, r, r.label()).changed();
                        }
                    });
                });
                changed |= theme::toggle(ui, &mut self.player.hwaccel, "Hardware-accelerated decoding").changed();
                if changed {
                    self.player.reload_settings();
                }
                theme::hint(ui, "Lower render sizes use less CPU; files are never modified.");
                ui.add_space(8.0);
                theme::section_title(ui, "Engine");
                match ffmpeg::tools() {
                    Some(t) => {
                        ui.label(RichText::new(&t.version).color(p.ok));
                        theme::hint(ui, &t.ffmpeg.display().to_string());
                    }
                    None => engine_download_ui(ui),
                }
                ui.add_space(8.0);
                theme::section_title(ui, "About");
                ui.label(RichText::new(format!("VMPC {}", env!("CARGO_PKG_VERSION"))).color(p.strong));
                theme::hint(
                    ui,
                    "Uses FFmpeg (ffmpeg.org) as its codec engine, run as a separate program. FFmpeg is \
                     licensed under the GNU GPL/LGPL; see THIRD_PARTY_NOTICES.md next to vmpc.",
                );
                ui.add_space(8.0);
                theme::section_title(ui, "Shortcuts");
                egui::Grid::new("keys").num_columns(2).spacing(vec2(18.0, 4.0)).show(ui, |ui| {
                    for (k, d) in [
                        ("Space", "Play / pause"),
                        ("← / →", "Seek 5 s (Shift 1 s, Ctrl 30 s)"),
                        ("↑ / ↓", "Volume"),
                        ("M", "Mute"),
                        ("F / F11 / Esc", "Fullscreen"),
                        ("N / P", "Next / previous"),
                        ("E", "Next frame"),
                        ("S", "Snapshot to Pictures"),
                        ("L", "Loop off / all / one"),
                        ("+ / −", "Speed"),
                        ("Ctrl+O", "Open files"),
                        ("Ctrl+L", "Toggle playlist"),
                    ] {
                        ui.label(RichText::new(k).monospace().color(p.accent));
                        ui.label(RichText::new(d).color(p.text));
                        ui.end_row();
                    }
                });
            });
        self.show_settings = open;
    }

    // ----- player view --------------------------------------------------------------

    fn player_tab(&mut self, ctx: &egui::Context, fullscreen: bool) {
        let p = pal();
        let show_controls =
            !fullscreen || self.player.paused || self.last_pointer_move.elapsed() < Duration::from_millis(2500);
        if show_controls {
            egui::TopBottomPanel::bottom("controls")
                .frame(egui::Frame::none().fill(if fullscreen { Color32::from_black_alpha(0) } else { p.bg }).inner_margin(Margin {
                    left: 20.0,
                    right: 20.0,
                    top: 6.0,
                    bottom: 18.0,
                }))
                .show(ctx, |ui| self.controls(ui, ctx));
        }
        if self.show_playlist && !fullscreen {
            egui::SidePanel::right("playlist")
                .resizable(false)
                .exact_width(300.0)
                .frame(egui::Frame::none().fill(p.bg).inner_margin(Margin { left: 6.0, right: 20.0, top: 6.0, bottom: 8.0 }))
                .show(ctx, |ui| self.playlist_panel(ui));
        }
        let margin = if fullscreen { Margin::same(0.0) } else { Margin { left: 20.0, right: 14.0, top: 10.0, bottom: 8.0 } };
        let fill = if fullscreen { Color32::BLACK } else { p.bg };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(fill).inner_margin(margin))
            .show(ctx, |ui| self.screen(ui, fullscreen));
        if fullscreen && !show_controls {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
    }

    fn screen(&mut self, ui: &mut Ui, fullscreen: bool) {
        let p = pal();
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click());
        let radius = if fullscreen { 0.0 } else { 24.0 };
        if !fullscreen {
            theme::paint_inset(ui.painter(), rect, radius, 8.0);
        }
        if resp.double_clicked() && self.player.is_loaded() {
            self.toggle_fullscreen(ui.ctx());
        }

        match &self.player.state {
            LoadState::Idle => self.welcome(ui, rect),
            LoadState::Loading => {
                let c = rect.center();
                ui.put(Rect::from_center_size(c, Vec2::splat(42.0)), egui::Spinner::new().size(42.0).color(p.accent));
                let name = self.player.source.as_deref().map(util::display_name).unwrap_or_default();
                ui.painter().text(c + vec2(0.0, 48.0), Align2::CENTER_CENTER, format!("Opening {name}…"), FontId::proportional(15.0), p.muted);
            }
            LoadState::Failed(e) => {
                let e = e.clone();
                let c = rect.center();
                ui.painter().text(c - vec2(0.0, 30.0), Align2::CENTER_CENTER, "Could not open this media", FontId::proportional(18.0), p.danger);
                ui.painter().text(c, Align2::CENTER_CENTER, e, FontId::proportional(13.0), p.muted);
                let btn = Rect::from_center_size(c + vec2(0.0, 50.0), vec2(170.0, 40.0));
                if ui.put(btn, |ui: &mut Ui| theme::button_icon(ui, Icon::Folder, "Open another", true)).clicked() {
                    self.open_dialog();
                }
            }
            LoadState::Ready => {
                if let Some(tex) = &self.player.texture {
                    let size = tex.size_vec2();
                    let inner = if fullscreen { rect } else { rect.shrink(10.0) };
                    let scale = (inner.width() / size.x).min(inner.height() / size.y);
                    let img = Rect::from_center_size(inner.center(), size * scale);
                    let r = if fullscreen { 0.0 } else { 16.0 };
                    theme::rounded_image(ui.painter(), img, r, tex.id());
                } else if !self.player.has_video() {
                    self.audio_visual(ui, rect);
                }
            }
        }
    }

    fn welcome(&mut self, ui: &mut Ui, rect: Rect) {
        let p = pal();
        let c = rect.center();
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let zone_size = vec2((rect.width() - 80.0).clamp(280.0, 620.0), (rect.height() - 170.0).clamp(200.0, 300.0));
        let zone_rect = Rect::from_center_size(c - vec2(0.0, 50.0), zone_size);
        let zone = ui.put(zone_rect, |ui: &mut Ui| {
            theme::drop_zone(ui, zone_size, "Drop any video, audio or image", "MP4 · MKV · AVI · MOV · WebM · MP3 · FLAC · and hundreds more", hovering)
        });
        if zone.clicked() {
            self.open_dialog();
        }
        let row = Rect::from_center_size(pos2(c.x, zone_rect.bottom() + 52.0), vec2(zone_size.x, 44.0));
        let mut play_url = false;
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(row), |ui| {
            ui.horizontal_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Globe, p.text);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.url_input)
                        .hint_text("Open network stream: https://…, rtsp://…, .m3u8")
                        .desired_width((zone_size.x - 140.0).max(120.0))
                        .margin(vec2(10.0, 8.0)),
                );
                play_url |= field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                play_url |= theme::button_icon(ui, Icon::Play, "Stream", true).clicked();
            });
        });
        if play_url && !self.url_input.trim().is_empty() {
            let url = self.url_input.trim().to_string();
            self.url_input.clear();
            self.enqueue(vec![url], true);
        }
        if ffmpeg::tools().is_none() {
            let area = Rect::from_center_size(pos2(c.x, row.bottom() + 46.0), vec2(zone_size.x, 70.0));
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(area), |ui| {
                ui.vertical_centered(|ui| engine_download_ui(ui));
            });
        }
    }

    fn audio_visual(&mut self, ui: &mut Ui, rect: Rect) {
        let p = pal();
        let target = self.player.level().min(0.5) * 2.0;
        self.level += (target - self.level) * 0.25;
        let c = rect.center() - vec2(0.0, 24.0);
        let base = (rect.height() * 0.42).clamp(120.0, 300.0);
        for i in (1..=3).rev() {
            let r = base / 2.0 + i as f32 * (10.0 + 26.0 * self.level);
            let alpha = 0.10 + 0.10 * self.level;
            ui.painter().circle_stroke(c, r, egui::Stroke::new(2.0_f32, p.accent.gamma_multiply(alpha / i as f32 * 2.0)));
        }
        let disc = Rect::from_center_size(c, Vec2::splat(base));
        theme::paint_raised(ui.painter(), disc, base / 2.0, 10.0);
        if let Some(cover) = &self.player.cover {
            theme::rounded_image(ui.painter(), disc.shrink(12.0), base / 2.0 - 12.0, cover.id());
        } else {
            icons::paint_logo(ui.painter(), disc.shrink(base * 0.22));
        }
        let name = self.player.source.as_deref().map(util::display_name).unwrap_or_default();
        ui.painter().text(pos2(c.x, disc.bottom() + 34.0), Align2::CENTER_CENTER, name, FontId::proportional(18.0), p.strong);
        if let Some(err) = self.player.audio_error() {
            ui.painter().text(pos2(c.x, disc.bottom() + 58.0), Align2::CENTER_CENTER, format!("Audio: {err}"), FontId::proportional(13.0), p.danger);
        }
    }

    fn controls(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let p = pal();
        let loaded = self.player.is_loaded();
        let duration = self.player.duration();
        let pos = self.player.position();
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            // Seek row
            ui.horizontal(|ui| {
                let shown = match (self.seek_drag, duration) {
                    (Some(v), Some(d)) => v as f64 * d,
                    _ => pos,
                };
                ui.label(RichText::new(util::fmt_time(if loaded { shown } else { f64::NAN })).monospace().color(p.text));
                let w = ui.available_width() - 70.0;
                let mut v = match (self.seek_drag, duration) {
                    (Some(v), _) => v,
                    (None, Some(d)) if d > 0.0 => (pos / d) as f32,
                    _ => 0.0,
                };
                let resp = theme::slider(ui, &mut v, w, 8.0, None);
                if loaded && duration.is_some() && !self.player.is_still() {
                    if resp.is_pointer_button_down_on() || resp.dragged() {
                        self.seek_drag = Some(v);
                    }
                    if resp.drag_stopped() || resp.clicked() {
                        if let Some(d) = duration {
                            self.player.seek(v as f64 * d);
                        }
                        self.seek_drag = None;
                    }
                } else {
                    self.seek_drag = None;
                }
                let dur_text = match duration {
                    Some(d) if loaded => util::fmt_time(d),
                    _ if loaded => "LIVE".into(),
                    _ => "--:--".into(),
                };
                ui.label(RichText::new(dur_text).monospace().color(p.text));
            });
            ui.add_space(2.0);
            // Buttons row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if theme::icon_button(ui, Icon::Prev, 40.0, false, "Previous (P)").clicked() {
                    self.prev();
                }
                if theme::icon_button(ui, Icon::Stop, 40.0, false, "Stop").clicked() {
                    self.player.stop();
                    self.current = None;
                }
                let (icon, tip) = if self.player.paused || !loaded { (Icon::Play, "Play (Space)") } else { (Icon::Pause, "Pause (Space)") };
                if theme::hero_button(ui, icon, 58.0, tip).clicked() {
                    if loaded {
                        self.player.toggle_pause();
                    } else if let Some(i) = self.current.or((!self.playlist.is_empty()).then_some(0)) {
                        self.play_index(i);
                    } else {
                        self.open_dialog();
                    }
                }
                if theme::icon_button(ui, Icon::Next, 40.0, false, "Next (N)").clicked() {
                    self.next();
                }
                if theme::icon_button(ui, Icon::Step, 34.0, false, "Next frame (E)").clicked() {
                    self.player.step_frame();
                }
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    let title = self.player.source.as_deref().map(util::display_name).unwrap_or_else(|| "Nothing playing".into());
                    let max_w = (ui.available_width() - 440.0).max(80.0);
                    ui.add(egui::Label::new(RichText::new(title).color(p.strong).size(15.0)).truncate())
                        .on_hover_text(self.player.source.clone().unwrap_or_default());
                    ui.set_max_width(max_w);
                    if let Some(info) = &self.player.info {
                        ui.add(egui::Label::new(RichText::new(info.summary()).size(11.5).color(p.muted)).truncate());
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    if theme::icon_button(ui, Icon::Fullscreen, 38.0, fs, "Fullscreen (F)").clicked() {
                        self.toggle_fullscreen(ctx);
                    }
                    if theme::icon_button(ui, Icon::Playlist, 38.0, self.show_playlist, "Playlist (Ctrl+L)").clicked() {
                        self.show_playlist = !self.show_playlist;
                    }
                    let mut vol = self.player.volume;
                    if theme::slider(ui, &mut vol, 110.0, 6.0, None).changed() {
                        self.player.set_volume(vol);
                        if self.player.muted {
                            self.player.set_muted(false);
                        }
                    }
                    let vicon = if self.player.muted || self.player.volume == 0.0 { Icon::Mute } else { Icon::Volume };
                    if theme::icon_button(ui, vicon, 38.0, self.player.muted, "Mute (M)").clicked() {
                        self.player.set_muted(!self.player.muted);
                    }
                    if theme::icon_button(ui, Icon::Camera, 38.0, false, "Snapshot (S)").clicked() {
                        self.snapshot();
                    }
                    let speed = self.player.speed;
                    egui::ComboBox::from_id_salt("speed")
                        .width(70.0)
                        .selected_text(format!("{speed}×"))
                        .show_ui(ui, |ui| {
                            for s in [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0] {
                                if ui.selectable_label((speed - s).abs() < 1e-6, format!("{s}×")).clicked() {
                                    self.player.set_speed(s);
                                }
                            }
                        });
                    let loop_tip = match self.loop_mode {
                        LoopMode::Off => "Loop: off (L)",
                        LoopMode::All => "Loop: playlist (L)",
                        LoopMode::One => "Loop: this item (L)",
                    };
                    let r = theme::icon_button(ui, Icon::Loop, 38.0, self.loop_mode != LoopMode::Off, loop_tip);
                    if self.loop_mode == LoopMode::One {
                        ui.painter().text(r.rect.right_top() + vec2(-6.0, 6.0), Align2::CENTER_CENTER, "1", FontId::proportional(11.0), p.accent);
                    }
                    if r.clicked() {
                        self.cycle_loop();
                    }
                });
            });
        });
    }

    fn playlist_panel(&mut self, ui: &mut Ui) {
        let p = pal();
        let mut play = None;
        let mut remove = None;
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height() - 4.0);
            ui.horizontal(|ui| {
                theme::section_title(ui, "Playlist");
                theme::chip(ui, &self.playlist.len().to_string(), p.accent);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(ui, Icon::Trash, 32.0, false, "Clear playlist").clicked() {
                        self.playlist.clear();
                        self.current = None;
                    }
                    if theme::icon_button(ui, Icon::Plus, 32.0, false, "Add files").clicked() {
                        if let Some(files) = rfd::FileDialog::new().pick_files() {
                            let play_now = self.playlist.is_empty();
                            self.enqueue(files.into_iter().map(|f| f.to_string_lossy().into_owned()).collect(), play_now);
                        }
                    }
                });
            });
            ui.add_space(4.0);
            if self.playlist.is_empty() {
                theme::hint(ui, "Drop files anywhere on the player to queue them.");
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (i, src) in self.playlist.iter().enumerate() {
                    let current = self.current == Some(i);
                    let row_h = 40.0;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                    if current {
                        theme::paint_inset(ui.painter(), rect, 12.0, 3.0);
                    } else if resp.hovered() {
                        ui.painter().extend(theme::raised_shapes(rect, 12.0, 2.5, p.bg));
                    }
                    let icon = if util::is_url(src) {
                        Icon::Globe
                    } else if current && !self.player.paused {
                        Icon::Volume
                    } else {
                        Icon::Play
                    };
                    let color = if current { p.accent } else { p.text };
                    icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(13.0)), icon, color);
                    let name = util::display_name(src);
                    let galley = ui.painter().layout(name, FontId::proportional(13.5), if current { p.strong } else { p.text }, rect.width() - 80.0);
                    let row0 = galley.rows.first().map(|r| r.rect.height()).unwrap_or(16.0);
                    ui.painter().with_clip_rect(rect.shrink2(vec2(34.0, 0.0))).galley(
                        pos2(rect.left() + 34.0, rect.center().y - row0 / 2.0),
                        galley,
                        p.text,
                    );
                    let close = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(22.0));
                    let close_resp = ui.interact(close, ui.id().with(("pl-x", i)), Sense::click());
                    if resp.hovered() || close_resp.hovered() {
                        icons::paint(ui.painter(), close.shrink(6.0), Icon::Close, if close_resp.hovered() { p.danger } else { p.muted });
                    }
                    if close_resp.clicked() {
                        remove = Some(i);
                    } else if resp.double_clicked() || resp.clicked() {
                        play = Some(i);
                    }
                    ui.add_space(2.0);
                }
            });
        });
        if let Some(i) = remove {
            self.playlist.remove(i);
            match self.current {
                Some(c) if c == i => {
                    self.current = None;
                    self.player.stop();
                }
                Some(c) if c > i => self.current = Some(c - 1),
                _ => {}
            }
        }
        if let Some(i) = play {
            if self.current != Some(i) {
                self.play_index(i);
            }
        }
    }

    // ----- overlays ---------------------------------------------------------------

    fn overlays(&mut self, ctx: &egui::Context) {
        let p = pal();
        let screen = ctx.screen_rect();
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop-overlay")));
            painter.rect_filled(screen, 0.0, p.bg.gamma_multiply(0.75));
            let card = Rect::from_center_size(screen.center(), vec2(360.0, 150.0));
            painter.extend(theme::raised_shapes(card, 24.0, 10.0, p.bg));
            icons::paint(&painter, Rect::from_center_size(card.center() - vec2(0.0, 22.0), Vec2::splat(38.0)), Icon::Upload, p.accent);
            let text = match self.tab {
                0 => "Drop to play",
                1 => "Drop to add to conversion",
                _ => "Drop video or images",
            };
            painter.text(card.center() + vec2(0.0, 36.0), Align2::CENTER_CENTER, text, FontId::proportional(18.0), p.strong);
        }
        if let Some((text, at)) = &self.toast {
            let age = at.elapsed().as_secs_f32();
            if age > 3.5 {
                self.toast = None;
            } else {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("toast")));
                let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(13.5), p.strong);
                let size = galley.size() + vec2(32.0, 20.0);
                let rect = Rect::from_center_size(pos2(screen.center().x, screen.top() + 100.0), size);
                painter.extend(theme::raised_shapes(rect, size.y / 2.0, 6.0, p.bg));
                painter.galley(rect.min + vec2(16.0, 10.0), galley, p.strong);
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
    }
}

/// "FFmpeg missing" notice with a one-click background download.
fn engine_download_ui(ui: &mut Ui) {
    let p = pal();
    match ffmpeg::download_state() {
        ffmpeg::Download::Running => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().color(p.accent));
                ui.label(RichText::new("Downloading the FFmpeg engine (~100 MB)…").color(p.text));
            });
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
        state => {
            ui.label(RichText::new("The FFmpeg engine is missing — VMPC needs it to play and convert.").color(p.danger));
            if let ffmpeg::Download::Failed(e) = state {
                theme::hint(ui, &format!("Download failed: {e}"));
            }
            if theme::button_icon(ui, Icon::Upload, "Download engine automatically", true).clicked() {
                ffmpeg::start_download(ui.ctx().clone());
            }
            if cfg!(not(windows)) {
                theme::hint(ui, "…or install it with your package manager, e.g. sudo apt install ffmpeg");
            }
        }
    }
}

impl App {
    fn current_settings(&self) -> Settings {
        Settings {
            dark: self.dark,
            volume: self.player.volume,
            muted: self.player.muted,
            max_height: self.player.max_res.height(),
            hwaccel: self.player.hwaccel,
            loop_mode: match self.loop_mode {
                LoopMode::Off => 0,
                LoopMode::All => 1,
                LoopMode::One => 2,
            },
            show_playlist: self.show_playlist,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Persist preference changes once they settle (not on every slider-drag frame).
        let current = self.current_settings();
        if current == self.saved {
            self.settings_dirty = None;
        } else {
            let since = *self.settings_dirty.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_millis(500) {
                settings::save(&current);
                self.saved = current;
                self.settings_dirty = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(600));
            }
        }
        self.handle_drops(ctx);
        self.handle_keys(ctx);
        let repaint = self.player.update(ctx);
        if self.player.take_ended_event() {
            let has_next = self.current.is_some_and(|i| i + 1 < self.playlist.len());
            match self.loop_mode {
                // Un-pausing an ended item restarts it from the beginning.
                LoopMode::One => self.player.set_paused(false),
                LoopMode::All if self.playlist.len() <= 1 => self.player.set_paused(false),
                LoopMode::All => self.next(),
                LoopMode::Off if has_next => self.next(),
                // Otherwise stay on the last frame; pressing play restarts.
                LoopMode::Off => {}
            }
        }
        let title = match self.player.source.as_deref() {
            Some(s) => format!("{} — VMPC", util::display_name(s)),
            None => "VMPC — Video Media Player Converter".into(),
        };
        // Only on change: setting the title triggers a window event and thus another repaint.
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let fullscreen_player = fullscreen && self.tab == 0;
        if !fullscreen_player {
            self.header(ctx);
        }
        let p = pal();
        match self.tab {
            0 => self.player_tab(ctx, fullscreen_player),
            1 => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(p.bg).inner_margin(Margin::symmetric(18.0, 10.0)))
                    .show(ctx, |ui| self.converter.ui(ui, &mut self.queue));
            }
            _ => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(p.bg).inner_margin(Margin::symmetric(18.0, 10.0)))
                    .show(ctx, |ui| self.frames.ui(ui, &mut self.queue));
            }
        }
        if self.show_settings {
            self.settings_window(ctx);
        }
        self.overlays(ctx);

        if let Some(d) = repaint {
            ctx.request_repaint_after(d);
        }
        if self.queue.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if fullscreen_player && !self.player.paused {
            // Keep checking so the auto-hidden controls come back on mouse move.
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}
