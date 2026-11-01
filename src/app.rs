//! Application shell. One workspace: a header with the logo (which opens the main
//! menu drawer), the preview stage in the middle, transport controls for playable
//! media, and a tools inspector that only offers what fits the detected media type.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{pos2, vec2, Align, Align2, Color32, FontId, Id, Key, Layout, Margin, Rect, RichText, Sense, Ui, Vec2};

use crate::converter::{self, Converter};
use crate::ffmpeg::{self, MediaInfo, MediaKind};
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Convert,
    Frames,
    ToVideo,
    Queue,
}

impl Tool {
    fn item(self) -> (Icon, &'static str) {
        match self {
            Tool::Convert => (Icon::Convert, "Convert"),
            Tool::Frames => (Icon::Frames, "Frames"),
            Tool::ToVideo => (Icon::Screen, "To video"),
            Tool::Queue => (Icon::Playlist, "Queue"),
        }
    }
}

/// A loaded file or stream and its probe result.
struct Item {
    source: String,
    info: Option<Result<MediaInfo, String>>,
}

impl Item {
    fn kind(&self) -> Option<MediaKind> {
        self.info.as_ref().and_then(|i| i.as_ref().ok()).map(|i| i.kind())
    }
}

const INSPECTOR_W: f32 = 380.0;

pub struct App {
    dark: bool,
    player: Player,
    library: Vec<Item>,
    current: Option<usize>,
    probe_tx: Sender<(String, Result<MediaInfo, String>)>,
    probe_rx: Receiver<(String, Result<MediaInfo, String>)>,
    loop_mode: LoopMode,
    seek_drag: Option<f32>,
    url_input: String,
    show_url: bool,
    converter: Converter,
    frames: Frames,
    queue: Queue,
    toast: Option<(String, Instant)>,
    last_pointer_move: Instant,
    menu_open: bool,
    show_settings: bool,
    tools_open: bool,
    tool: Tool,
    level: f32,
    title: String,
    frame: u64,
    /// When the media now on stage first appeared (drives the reveal animation).
    revealed: Option<(String, Instant)>,
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
        let (probe_tx, probe_rx) = mpsc::channel();
        let mut app = Self {
            dark: saved.dark,
            player,
            library: Vec::new(),
            current: None,
            probe_tx,
            probe_rx,
            loop_mode: match saved.loop_mode {
                1 => LoopMode::All,
                2 => LoopMode::One,
                _ => LoopMode::Off,
            },
            seek_drag: None,
            url_input: String::new(),
            show_url: false,
            converter: Converter::default(),
            frames: Frames::default(),
            queue: Queue::new(cc.egui_ctx.clone()),
            toast: None,
            last_pointer_move: Instant::now(),
            menu_open: false,
            show_settings: false,
            tools_open: saved.tools_open,
            tool: Tool::Convert,
            level: 0.0,
            title: String::new(),
            frame: 0,
            revealed: None,
            saved,
            settings_dirty: None,
        };
        if !files.is_empty() {
            app.enqueue(&cc.egui_ctx, files, true);
        }
        app
    }

    // ----- library ------------------------------------------------------------

    fn enqueue(&mut self, ctx: &egui::Context, sources: Vec<String>, play_first: bool) {
        let first_new = self.library.len();
        for source in sources {
            if self.library.iter().any(|i| i.source == source) {
                continue;
            }
            let (tx, src, ctx) = (self.probe_tx.clone(), source.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send((src.clone(), ffmpeg::probe(&src)));
                ctx.request_repaint();
            });
            self.library.push(Item { source, info: None });
        }
        // A batch of images is most likely an image sequence: offer "To video" right away.
        let added = &self.library[first_new..];
        if added.len() > 1 && added.iter().all(|i| util::is_image_path(std::path::Path::new(&i.source))) {
            self.tool = Tool::ToVideo;
            self.tools_open = true;
        }
        if play_first && self.library.len() > first_new {
            self.play_index(first_new);
        }
    }

    fn play_index(&mut self, i: usize) {
        if let Some(item) = self.library.get(i) {
            self.current = Some(i);
            self.seek_drag = None;
            self.player.open(item.source.clone());
        }
    }

    fn remove_item(&mut self, i: usize) {
        self.library.remove(i);
        match self.current {
            Some(c) if c == i => {
                self.current = None;
                self.player.stop();
            }
            Some(c) if c > i => self.current = Some(c - 1),
            _ => {}
        }
    }

    fn next(&mut self) {
        match self.current {
            Some(i) if i + 1 < self.library.len() => self.play_index(i + 1),
            Some(_) if self.loop_mode == LoopMode::All && !self.library.is_empty() => self.play_index(0),
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

    fn open_files(&mut self, ctx: &egui::Context) {
        if let Some(files) = rfd::FileDialog::new().set_title("Open media").pick_files() {
            let files = files.into_iter().map(|f| f.to_string_lossy().into_owned()).collect();
            self.enqueue(ctx, files, true);
        }
    }

    fn open_folder(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Open folder").pick_folder() {
            let files = expand(&[dir]);
            self.enqueue(ctx, files, true);
        }
    }

    /// The media in the preview, once it has been analysed.
    fn current_media(&self) -> Option<(String, MediaInfo)> {
        let source = self.player.source.clone()?;
        let info = self.player.info.clone()?;
        Some((source, info))
    }

    fn images(&self) -> Vec<PathBuf> {
        self.library
            .iter()
            .filter(|i| i.kind() == Some(MediaKind::Image) && !util::is_url(&i.source))
            .map(|i| PathBuf::from(&i.source))
            .collect()
    }

    fn tools_for(&self, kind: MediaKind) -> Vec<Tool> {
        let mut tools = vec![Tool::Convert];
        match kind {
            MediaKind::Video => tools.push(Tool::Frames),
            MediaKind::Image if self.images().len() > 1 => tools.push(Tool::ToVideo),
            _ => {}
        }
        tools.push(Tool::Queue);
        tools
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

    fn cycle_loop(&mut self) {
        self.loop_mode = match self.loop_mode {
            LoopMode::Off => LoopMode::All,
            LoopMode::All => LoopMode::One,
            LoopMode::One => LoopMode::Off,
        };
        let text = match self.loop_mode {
            LoopMode::Off => "Loop off",
            LoopMode::All => "Loop all files",
            LoopMode::One => "Loop current file",
        };
        self.toast(text);
    }

    // ----- input --------------------------------------------------------------

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            let files = expand(&dropped);
            self.enqueue(ctx, files, true);
            self.menu_open = false;
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
                Key::O if mods.command => self.open_files(ctx),
                Key::L if mods.command => self.menu_open = !self.menu_open,
                Key::Escape if self.menu_open => self.menu_open = false,
                Key::Escape => ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false)),
                Key::T => self.tools_open = !self.tools_open,
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
                Key::L => self.cycle_loop(),
                Key::Plus | Key::Equals => self.player.set_speed(self.player.speed * 1.25),
                Key::Minus => self.player.set_speed(self.player.speed / 1.25),
                _ => {}
            }
        }
    }

    // ----- header ---------------------------------------------------------------

    fn header(&mut self, ctx: &egui::Context, boot: f32) {
        let p = pal();
        egui::TopBottomPanel::top("header")
            .show_separator_line(false)
            .exact_height(72.0)
            .frame(egui::Frame::none().fill(p.bg).inner_margin(Margin { left: 20.0, right: 20.0, top: 12.0, bottom: 6.0 }))
            .show(ctx, |ui| {
                ui.set_opacity(boot);
                ui.horizontal_centered(|ui| {
                    if theme::logo_button(ui, 50.0, self.menu_open).on_hover_text("Menu").clicked() {
                        self.menu_open = !self.menu_open;
                    }
                    ui.add_space(10.0);
                    if let Some((source, info)) = self.current_media() {
                        let t = theme::ease(ctx, Id::new(("title", &source)), self.frame > 2, 0.4);
                        ui.vertical(|ui| {
                            ui.set_opacity(t);
                            ui.add_space(4.0 + 6.0 * (1.0 - t));
                            ui.add(egui::Label::new(RichText::new(util::display_name(&source)).size(16.0).strong().color(p.strong)).truncate())
                                .on_hover_text(info.summary());
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                badge(ui, info.kind().label(), true);
                                if let Some(v) = &info.video {
                                    badge(ui, &format!("{}×{}", v.width, v.height), false);
                                }
                                if let Some(d) = info.duration.filter(|_| info.kind() != MediaKind::Image) {
                                    badge(ui, &util::fmt_time(d), false);
                                }
                                badge(ui, &codec_label(&info), false);
                            });
                        });
                    } else if let Some(source) = self.player.source.as_deref() {
                        ui.label(RichText::new(util::display_name(source)).size(16.0).color(p.muted));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.current_media().is_some()
                            && theme::icon_button(ui, Icon::Convert, 42.0, self.tools_open, "Tools (T)").clicked()
                        {
                            self.tools_open = !self.tools_open;
                        }
                        if self.queue.is_busy() {
                            ui.add_space(6.0);
                            let progress = self.queue.snapshot().iter().find(|j| j.status == crate::jobs::Status::Running).map(|j| j.progress);
                            let text = match progress {
                                Some(f) => format!("Converting {:.0}%", f * 100.0),
                                None => "Converting…".into(),
                            };
                            let r = ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.accent)).sense(Sense::click()));
                            ui.add(egui::Spinner::new().size(16.0).color(p.accent));
                            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.tools_open = true;
                                self.tool = Tool::Queue;
                            }
                        }
                    });
                });
            });
    }

    // ----- main menu drawer ---------------------------------------------------------

    fn menu(&mut self, ctx: &egui::Context) {
        let t = theme::ease(ctx, Id::new("menu-anim"), self.menu_open, 0.32);
        if t <= 0.001 {
            return;
        }
        let p = pal();
        let screen = ctx.screen_rect();
        let width = 330.0_f32.min(screen.width() - 40.0);
        let drawer = Rect::from_min_size(
            pos2(screen.left() + 14.0 - (width + 40.0) * (1.0 - t), screen.top() + 14.0),
            vec2(width, screen.height() - 28.0),
        );
        egui::Area::new(Id::new("menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                // Dimmed backdrop; clicking it closes the menu.
                let backdrop = ui.allocate_rect(screen, Sense::click());
                ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha((110.0 * t) as u8));
                if backdrop.clicked() {
                    self.menu_open = false;
                }
                ui.painter().extend(theme::raised_shapes(drawer, 22.0, 10.0, p.bg));
                ui.allocate_rect(drawer, Sense::click()); // eats clicks on the drawer background
                let inner = drawer.shrink2(vec2(16.0, 16.0));
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| {
                    ui.set_opacity(t);
                    ui.horizontal(|ui| {
                        if theme::logo_button(ui, 46.0, true).on_hover_text("Close menu").clicked() {
                            self.menu_open = false;
                        }
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Video · Media · Player · Converter").size(12.0).color(p.muted));
                        });
                    });
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.menu_contents(ui));
                });
            });
    }

    fn menu_contents(&mut self, ui: &mut Ui) {
        let p = pal();
        let ctx = ui.ctx().clone();
        section(ui, "OPEN");
        if theme::menu_item(ui, Icon::Upload, "Open files…", Some("Video, audio or images · Ctrl+O"), false).clicked() {
            self.menu_open = false;
            self.open_files(&ctx);
        }
        if theme::menu_item(ui, Icon::Folder, "Open folder…", Some("Everything inside, e.g. an image sequence"), false).clicked() {
            self.menu_open = false;
            self.open_folder(&ctx);
        }
        if theme::menu_item(ui, Icon::Globe, "Open network stream…", Some("https, HLS (.m3u8), RTSP, RTMP"), self.show_url).clicked() {
            self.show_url = !self.show_url;
        }
        let url_t = theme::ease(&ctx, Id::new("url-row"), self.show_url, 0.2);
        if url_t > 0.01 {
            ui.horizontal(|ui| {
                ui.set_opacity(url_t);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.url_input)
                        .hint_text("Paste a stream URL")
                        .desired_width(ui.available_width() - 80.0)
                        .margin(vec2(8.0, 6.0)),
                );
                let go = theme::button(ui, "Play", true).clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                if go && !self.url_input.trim().is_empty() {
                    let url = self.url_input.trim().to_string();
                    self.url_input.clear();
                    self.show_url = false;
                    self.menu_open = false;
                    self.enqueue(&ctx, vec![url], true);
                }
            });
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            section(ui, &format!("LIBRARY · {}", self.library.len()));
            if !self.library.is_empty() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let clear = ui.add(egui::Label::new(RichText::new("Clear").size(12.0).color(p.muted)).sense(Sense::click()));
                    if clear.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.library.clear();
                        self.current = None;
                        self.player.stop();
                    }
                });
            }
        });
        if self.library.is_empty() {
            theme::hint(ui, "Files you open or drop appear here.");
        }
        let mut play = None;
        let mut remove = None;
        for (i, item) in self.library.iter().enumerate() {
            let icon = match (util::is_url(&item.source), item.kind()) {
                (true, _) => Icon::Globe,
                (_, Some(MediaKind::Video)) => Icon::Screen,
                (_, Some(MediaKind::Audio)) => Icon::Music,
                (_, Some(MediaKind::Image)) => Icon::Image,
                _ => Icon::Folder,
            };
            let detail = match &item.info {
                None => "Analysing…".to_string(),
                Some(Err(_)) => "Can't be opened".to_string(),
                Some(Ok(info)) => match info.kind() {
                    MediaKind::Image => format!("Image · {}", codec_label(info)),
                    k => format!("{} · {}", k.label().to_lowercase(), util::fmt_time(info.duration.unwrap_or(0.0))),
                },
            };
            let resp = theme::menu_item(ui, icon, &theme::truncate(&util::display_name(&item.source), 30), Some(&detail), self.current == Some(i));
            let close = Rect::from_center_size(pos2(resp.rect.right() - 18.0, resp.rect.center().y), Vec2::splat(22.0));
            let close_resp = ui.interact(close, ui.id().with(("lib-x", i)), Sense::click());
            if resp.hovered() || close_resp.hovered() {
                icons::paint(ui.painter(), close.shrink(6.0), Icon::Close, if close_resp.hovered() { p.danger } else { p.muted });
            }
            if close_resp.clicked() {
                remove = Some(i);
            } else if resp.clicked() {
                play = Some(i);
            }
        }
        if let Some(i) = remove {
            self.remove_item(i);
        } else if let Some(i) = play {
            if self.current != Some(i) {
                self.play_index(i);
            }
            self.menu_open = false;
        }

        ui.add_space(6.0);
        section(ui, "TOOLS");
        let kind = self.current_media().map(|(_, i)| i.kind());
        let tools: Vec<(Tool, &str, String)> = vec![
            (Tool::Convert, "Convert", kind.map(|k| format!("Formats that fit this {}", k.label().to_lowercase())).unwrap_or_else(|| "Open a file first".into())),
            (Tool::Frames, "Video → images", "Image sequence at any fps".into()),
            (Tool::ToVideo, "Images → video", format!("{} images loaded", self.images().len())),
            (Tool::Queue, "Queue", format!("{} conversion(s)", self.queue.snapshot().len())),
        ];
        for (tool, label, detail) in tools {
            let available = match tool {
                Tool::Convert => kind.is_some(),
                Tool::Frames => kind == Some(MediaKind::Video),
                Tool::ToVideo => self.images().len() > 1,
                Tool::Queue => true,
            };
            let (icon, _) = tool.item();
            let detail = if available || tool != Tool::Frames { detail } else { "Needs a video in the preview".into() };
            let resp = theme::menu_item(ui, icon, label, Some(&detail), false);
            if resp.clicked() {
                if available {
                    self.tool = tool;
                    self.tools_open = true;
                    self.menu_open = false;
                } else if tool == Tool::ToVideo {
                    self.menu_open = false;
                    self.open_folder(&ctx);
                } else {
                    self.toast(detail.clone());
                }
            }
        }

        ui.add_space(6.0);
        section(ui, "APP");
        if theme::menu_item(ui, Icon::Gear, "Settings", Some("Appearance, playback, engine"), self.show_settings).clicked() {
            self.show_settings = true;
            self.menu_open = false;
        }
        if theme::menu_item(ui, Icon::Fullscreen, "Fullscreen", Some("F or F11"), false).clicked() {
            self.menu_open = false;
            self.toggle_fullscreen(&ctx);
        }
        ui.add_space(10.0);
        theme::hint(ui, &format!("VMPC {}", env!("CARGO_PKG_VERSION")));
    }

    // ----- settings -----------------------------------------------------------------

    fn settings_window(&mut self, ctx: &egui::Context) {
        let p = pal();
        let mut open = self.show_settings;
        egui::Window::new(RichText::new("Settings").strong().size(17.0).color(p.strong))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(egui::Frame::window(&ctx.style()).inner_margin(20.0).fill(p.bg))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(ctx.screen_rect().height() - 160.0).show(ui, |ui| {
                    section(ui, "APPEARANCE");
                    let mut theme_idx = usize::from(self.dark);
                    if theme::segmented(ui, &mut theme_idx, &[(Icon::Sun, "Light"), (Icon::Moon, "Dark")]) {
                        self.dark = theme_idx == 1;
                        theme::apply(ctx, self.dark);
                    }
                    ui.add_space(10.0);
                    section(ui, "PLAYBACK");
                    let mut changed = false;
                    converter::field(ui, "Max video render size", |ui| {
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
                    ui.add_space(10.0);
                    section(ui, "ENGINE");
                    match ffmpeg::tools() {
                        Some(t) => {
                            ui.label(RichText::new(&t.version).size(13.0).color(p.ok));
                            theme::hint(ui, &t.ffmpeg.display().to_string());
                        }
                        None => engine_download_ui(ui),
                    }
                    ui.add_space(10.0);
                    section(ui, "SHORTCUTS");
                    egui::Grid::new("keys").num_columns(2).spacing(vec2(18.0, 4.0)).show(ui, |ui| {
                        for (k, d) in [
                            ("Space", "Play / pause"),
                            ("← / →", "Seek 5 s (Shift 1 s, Ctrl 30 s)"),
                            ("↑ / ↓", "Volume"),
                            ("M", "Mute"),
                            ("F / F11 / Esc", "Fullscreen"),
                            ("N / P", "Next / previous file"),
                            ("E", "Next frame"),
                            ("S", "Snapshot to Pictures"),
                            ("L", "Loop off / all / one"),
                            ("+ / −", "Speed"),
                            ("T", "Show / hide tools"),
                            ("Ctrl+O", "Open files"),
                            ("Ctrl+L", "Menu"),
                        ] {
                            ui.label(RichText::new(k).monospace().color(p.accent));
                            ui.label(RichText::new(d).color(p.text));
                            ui.end_row();
                        }
                    });
                    ui.add_space(10.0);
                    section(ui, "ABOUT");
                    ui.label(RichText::new(format!("VMPC {}", env!("CARGO_PKG_VERSION"))).color(p.strong));
                    theme::hint(
                        ui,
                        "Uses FFmpeg (ffmpeg.org) as its codec engine, run as a separate program. FFmpeg is \
                         licensed under the GNU GPL/LGPL; see THIRD_PARTY_NOTICES.md next to vmpc.",
                    );
                });
            });
        self.show_settings = open;
    }

    // ----- workspace -----------------------------------------------------------------

    fn workspace(&mut self, ctx: &egui::Context, fullscreen: bool, boot: f32) {
        let p = pal();
        let playable = self.player.is_loaded() && !self.player.is_still();
        let show_controls =
            playable && (!fullscreen || self.player.paused || self.last_pointer_move.elapsed() < Duration::from_millis(2500));
        egui::TopBottomPanel::bottom("controls")
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(if fullscreen { Color32::BLACK } else { p.bg }).inner_margin(Margin {
                left: 20.0,
                right: 20.0,
                top: 4.0,
                bottom: 18.0,
            }))
            .show_animated(ctx, show_controls, |ui| self.controls(ui, ctx));

        let media = self.current_media();
        let inspector = !fullscreen && self.tools_open && media.is_some();
        egui::SidePanel::right("inspector")
            .show_separator_line(false)
            .resizable(false)
            .exact_width(INSPECTOR_W)
            .frame(egui::Frame::none().fill(p.bg).inner_margin(Margin { left: 4.0, right: 20.0, top: 8.0, bottom: 18.0 }))
            .show_animated(ctx, inspector, |ui| {
                if let Some(media) = &media {
                    self.inspector(ui, media);
                }
            });

        let margin = if fullscreen { Margin::same(0.0) } else { Margin { left: 20.0, right: 14.0, top: 8.0, bottom: 14.0 } };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(if fullscreen { Color32::BLACK } else { p.bg }).inner_margin(margin))
            .show(ctx, |ui| {
                ui.set_opacity(boot);
                self.stage(ui, fullscreen);
            });
        if fullscreen && playable && !show_controls {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
    }

    fn inspector(&mut self, ui: &mut Ui, media: &(String, MediaInfo)) {
        let kind = media.1.kind();
        let tools = self.tools_for(kind);
        if !tools.contains(&self.tool) {
            self.tool = Tool::Convert;
        }
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height() - 4.0);
            let items: Vec<(Icon, &str)> = tools.iter().map(|t| t.item()).collect();
            let mut sel = tools.iter().position(|t| *t == self.tool).unwrap_or(0);
            if theme::segmented(ui, &mut sel, &items) {
                self.tool = tools[sel];
            }
            ui.add_space(6.0);
            let same_kind: Vec<(String, MediaInfo)> = self
                .library
                .iter()
                .filter_map(|i| match &i.info {
                    Some(Ok(info)) if info.kind() == kind => Some((i.source.clone(), info.clone())),
                    _ => None,
                })
                .collect();
            let images = self.images();
            // Panels slide in when switching tools.
            let t = theme::ease(ui.ctx(), Id::new(("tool", self.tool as u8)), true, 0.25);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_opacity(t);
                ui.add_space(8.0 * (1.0 - t));
                match self.tool {
                    Tool::Convert => self.converter.panel(ui, &mut self.queue, (&media.0, &media.1), &same_kind),
                    Tool::Frames => self.frames.extract_panel(ui, &mut self.queue, &media.0, &media.1),
                    Tool::ToVideo => self.frames.assemble_panel(ui, &mut self.queue, &images),
                    Tool::Queue => converter::jobs_list(ui, &mut self.queue),
                }
            });
        });
    }

    fn stage(&mut self, ui: &mut Ui, fullscreen: bool) {
        let p = pal();
        let ctx = ui.ctx().clone();
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click());
        let radius = if fullscreen { 0.0 } else { 26.0 };
        let active = !matches!(self.player.state, LoadState::Idle);
        // The well darkens into a cinema stage once something is opened.
        let dim = theme::ease(&ctx, Id::new("stage-dim"), active, 0.45);
        if !fullscreen {
            theme::paint_inset(ui.painter(), rect, radius, 8.0);
            if dim > 0.0 {
                ui.painter().rect_filled(rect.shrink(1.5), radius - 1.0, p.stage.gamma_multiply(dim));
            }
        }
        if resp.double_clicked() && self.player.is_loaded() {
            self.toggle_fullscreen(&ctx);
        }

        match &self.player.state {
            LoadState::Idle => {
                ui.set_opacity(1.0 - dim);
                self.welcome(ui, rect);
            }
            LoadState::Loading => {
                let c = rect.center();
                ui.put(Rect::from_center_size(c, Vec2::splat(40.0)), egui::Spinner::new().size(40.0).color(p.accent));
                let name = self.player.source.as_deref().map(util::display_name).unwrap_or_default();
                ui.painter().text(c + vec2(0.0, 46.0), Align2::CENTER_CENTER, format!("Opening {name}…"), FontId::proportional(14.0), Color32::from_gray(170));
            }
            LoadState::Failed(e) => {
                let e = e.clone();
                let c = rect.center();
                ui.painter().text(c - vec2(0.0, 30.0), Align2::CENTER_CENTER, "This file can't be opened", FontId::proportional(18.0), p.danger);
                ui.painter().text(c, Align2::CENTER_CENTER, theme::truncate(&e, 90), FontId::proportional(13.0), Color32::from_gray(170));
                let btn = Rect::from_center_size(c + vec2(0.0, 50.0), vec2(170.0, 40.0));
                if ui.put(btn, |ui: &mut Ui| theme::button_icon(ui, Icon::Folder, "Open another", true)).clicked() {
                    self.open_files(&ctx);
                }
            }
            LoadState::Ready => {
                let source = self.player.source.clone().unwrap_or_default();
                if let Some(tex) = &self.player.texture {
                    // Reveal: fade + settle from 96 % scale when new media appears.
                    if self.revealed.as_ref().map(|(s, _)| s != &source).unwrap_or(true) {
                        self.revealed = Some((source.clone(), Instant::now()));
                    }
                    let elapsed = self.revealed.as_ref().map(|(_, t)| t.elapsed().as_secs_f32()).unwrap_or(1.0);
                    let k = egui::emath::easing::cubic_out((elapsed / 0.45).min(1.0));
                    if k < 1.0 {
                        ctx.request_repaint();
                    }
                    let size = tex.size_vec2();
                    let inner = if fullscreen { rect } else { rect.shrink(14.0) };
                    let scale = (inner.width() / size.x).min(inner.height() / size.y) * (0.96 + 0.04 * k);
                    let img = Rect::from_center_size(inner.center(), size * scale);
                    let r = if fullscreen { 0.0 } else { 14.0 };
                    theme::rounded_image(ui.painter(), img, r, tex.id(), k);
                } else if !self.player.has_video() {
                    self.audio_visual(ui, rect);
                }
            }
        }
    }

    fn welcome(&mut self, ui: &mut Ui, rect: Rect) {
        let p = pal();
        let ctx = ui.ctx().clone();
        let c = rect.center();
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let zone_size = vec2((rect.width() - 80.0).clamp(280.0, 640.0), (rect.height() - 150.0).clamp(220.0, 320.0));
        let zone_rect = Rect::from_center_size(c - vec2(0.0, 36.0), zone_size);
        let zone = ui.put(zone_rect, |ui: &mut Ui| {
            theme::drop_zone(ui, zone_size, "Drop a video, song or image", "or click to browse · it's detected automatically", hovering)
        });
        if zone.clicked() {
            self.open_files(&ctx);
        }
        let row = Rect::from_center_size(pos2(c.x, zone_rect.bottom() + 40.0), vec2(zone_size.x, 40.0));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(row), |ui| {
            ui.horizontal_centered(|ui| {
                let formats = "MP4 · MKV · MOV · AVI · WebM · MP3 · FLAC · WAV · PNG · JPG · GIF · and hundreds more";
                ui.add_space(((row.width() - 560.0) / 2.0).max(0.0));
                ui.label(RichText::new(formats).size(12.0).color(p.muted));
            });
        });
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
        let c = rect.center() - vec2(0.0, 20.0);
        let base = (rect.height() * 0.42).clamp(120.0, 300.0);
        for i in (1..=4).rev() {
            let r = base / 2.0 + i as f32 * (12.0 + 30.0 * self.level);
            let alpha = (0.08 + 0.22 * self.level) / i as f32;
            ui.painter().circle_stroke(c, r, egui::Stroke::new(2.0_f32, p.accent.gamma_multiply(alpha)));
        }
        let disc = Rect::from_center_size(c, Vec2::splat(base));
        theme::glow(ui.painter(), disc, base / 2.0, 0.4 + 0.6 * self.level);
        ui.painter().circle_filled(c, base / 2.0, Color32::from_gray(30));
        if let Some(cover) = &self.player.cover {
            theme::rounded_image(ui.painter(), disc.shrink(10.0), base / 2.0 - 10.0, cover.id(), 1.0);
        } else {
            icons::paint_logo_ex(ui.painter(), disc.shrink(base * 0.2), self.player.position() as f32 * 24.0);
        }
        if let Some(err) = self.player.audio_error() {
            ui.painter().text(pos2(c.x, disc.bottom() + 34.0), Align2::CENTER_CENTER, format!("Audio: {err}"), FontId::proportional(13.0), p.danger);
        }
    }

    fn controls(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let p = pal();
        let duration = self.player.duration();
        let pos = self.player.position();
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let shown = match (self.seek_drag, duration) {
                    (Some(v), Some(d)) => v as f64 * d,
                    _ => pos,
                };
                ui.label(RichText::new(util::fmt_time(shown)).monospace().color(p.text));
                let w = ui.available_width() - 70.0;
                let mut v = match (self.seek_drag, duration) {
                    (Some(v), _) => v,
                    (None, Some(d)) if d > 0.0 => (pos / d) as f32,
                    _ => 0.0,
                };
                let resp = theme::slider(ui, &mut v, w, 8.0, None);
                if duration.is_some() {
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
                let dur_text = duration.map(util::fmt_time).unwrap_or_else(|| "LIVE".into());
                ui.label(RichText::new(dur_text).monospace().color(p.text));
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if theme::icon_button(ui, Icon::Prev, 40.0, false, "Previous (P)").clicked() {
                    self.prev();
                }
                let (icon, tip) = if self.player.paused { (Icon::Play, "Play (Space)") } else { (Icon::Pause, "Pause (Space)") };
                if theme::hero_button(ui, icon, 58.0, tip).clicked() {
                    self.player.toggle_pause();
                }
                if theme::icon_button(ui, Icon::Next, 40.0, false, "Next (N)").clicked() {
                    self.next();
                }
                if self.player.has_video() && theme::icon_button(ui, Icon::Step, 34.0, false, "Next frame (E)").clicked() {
                    self.player.step_frame();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    if theme::icon_button(ui, Icon::Fullscreen, 38.0, fs, "Fullscreen (F)").clicked() {
                        self.toggle_fullscreen(ctx);
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
                    if self.player.has_video() && theme::icon_button(ui, Icon::Camera, 38.0, false, "Snapshot (S)").clicked() {
                        self.snapshot();
                    }
                    let speed = self.player.speed;
                    egui::ComboBox::from_id_salt("speed").width(64.0).selected_text(format!("{speed}×")).show_ui(ui, |ui| {
                        for s in [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0] {
                            if ui.selectable_label((speed - s).abs() < 1e-6, format!("{s}×")).clicked() {
                                self.player.set_speed(s);
                            }
                        }
                    });
                    let loop_tip = match self.loop_mode {
                        LoopMode::Off => "Loop: off (L)",
                        LoopMode::All => "Loop: all files (L)",
                        LoopMode::One => "Loop: this file (L)",
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

    // ----- overlays -------------------------------------------------------------------

    fn overlays(&mut self, ctx: &egui::Context) {
        let p = pal();
        let screen = ctx.screen_rect();
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let t = theme::ease(ctx, Id::new("drop-overlay"), hovering, 0.2);
        if t > 0.01 {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("drop-overlay")));
            painter.rect_filled(screen, 0.0, p.bg.gamma_multiply(0.8 * t));
            let card = Rect::from_center_size(screen.center(), vec2(340.0, 150.0) * (0.94 + 0.06 * t));
            theme::glow(&painter, card, 24.0, t);
            painter.extend(theme::raised_shapes(card, 24.0, 10.0 * t, p.bg));
            icons::paint(&painter, Rect::from_center_size(card.center() - vec2(0.0, 22.0), Vec2::splat(38.0)), Icon::Upload, p.accent.gamma_multiply(t));
            painter.text(card.center() + vec2(0.0, 36.0), Align2::CENTER_CENTER, "Drop to open", FontId::proportional(18.0), p.strong.gamma_multiply(t));
        }
        if let Some((text, at)) = &self.toast {
            let age = at.elapsed().as_secs_f32();
            if age > 3.5 {
                self.toast = None;
            } else {
                let fade = (age / 0.2).min(1.0).min((3.5 - age) / 0.3);
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("toast")));
                let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(13.5), p.strong.gamma_multiply(fade));
                let size = galley.size() + vec2(32.0, 20.0);
                let rect = Rect::from_center_size(pos2(screen.center().x, screen.top() + 96.0 - 8.0 * (1.0 - fade)), size);
                painter.extend(theme::raised_shapes(rect, size.y / 2.0, 6.0 * fade, p.bg.gamma_multiply(fade)));
                painter.galley(rect.min + vec2(16.0, 10.0), galley, p.strong);
                ctx.request_repaint_after(Duration::from_millis(30));
            }
        }
    }

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
            tools_open: self.tools_open,
        }
    }
}

/// Small caps section label.
fn section(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong().color(pal().muted));
}

/// Rounded info badge in the header; `accent` = orange media-type badge.
fn badge(ui: &mut Ui, text: &str, accent: bool) {
    let p = pal();
    let color = if accent { Color32::WHITE } else { p.text };
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(11.0), color);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(14.0, 6.0), Sense::hover());
    if accent {
        theme::gradient_rrect(ui.painter(), rect, rect.height() / 2.0, p.accent, p.accent2);
    } else {
        theme::paint_inset(ui.painter(), rect, rect.height() / 2.0, 1.5);
    }
    ui.painter().galley(rect.min + vec2(7.0, 3.0), galley, color);
}

fn codec_label(info: &MediaInfo) -> String {
    let mut parts = Vec::new();
    if let Some(v) = &info.video {
        parts.push(v.codec.to_uppercase());
    }
    if let Some(a) = &info.audio {
        parts.push(a.codec.to_uppercase());
    }
    parts.join(" + ")
}

/// Expands dropped/picked folders into their files (sorted naturally).
fn expand(paths: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(p)
                .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
                .unwrap_or_default();
            files.sort_by(|a, b| util::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
            out.extend(files.into_iter().map(|f| f.to_string_lossy().into_owned()));
        } else {
            out.push(p.to_string_lossy().into_owned());
        }
    }
    out
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

impl eframe::App for App {
    /// eframe clears to near-black by default, which shows through hairline gaps
    /// between panels; use the theme background instead.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        pal().bg.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame += 1;
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
        while let Ok((src, res)) = self.probe_rx.try_recv() {
            if let Some(item) = self.library.iter_mut().find(|i| i.source == src) {
                item.info = Some(res);
            }
        }
        self.handle_drops(ctx);
        self.handle_keys(ctx);
        let repaint = self.player.update(ctx);
        if self.player.take_ended_event() {
            let has_next = self.current.is_some_and(|i| i + 1 < self.library.len());
            match self.loop_mode {
                // Un-pausing an ended item restarts it from the beginning.
                LoopMode::One => self.player.set_paused(false),
                LoopMode::All if self.library.len() <= 1 => self.player.set_paused(false),
                LoopMode::All => self.next(),
                LoopMode::Off if has_next => self.next(),
                // Otherwise stay on the last frame; pressing play restarts.
                LoopMode::Off => {}
            }
        }
        let title = match self.player.source.as_deref() {
            Some(s) => format!("{} — VMPC", util::display_name(s)),
            None => "VMPC".into(),
        };
        // Only on change: setting the title triggers a window event and thus another repaint.
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        // Gentle fade-in of the whole UI on launch.
        let boot = theme::ease(ctx, Id::new("boot"), self.frame > 1, 0.6);
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if !fullscreen {
            self.header(ctx, boot);
        }
        self.workspace(ctx, fullscreen, boot);
        self.menu(ctx);
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
        if fullscreen && !self.player.paused {
            // Keep checking so the auto-hidden controls come back on mouse move.
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}
