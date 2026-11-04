//! Application shell, laid out like the Claude app: a collapsible sidebar (the logo
//! toggles it) with open actions and the library, a home page with a centred
//! "composer" to import media, the preview stage with playback controls, and a
//! slide-out bar listing every conversion that fits the detected media type.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{pos2, vec2, Align, Align2, Color32, FontId, Id, Key, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::catalog::{self, Classified, DataFormat, Handler};
use crate::converter::{self, Converter, Subject, Target};
use crate::ffmpeg::{self, MediaInfo, MediaKind};
use crate::formats::{self, Kind, FORMATS, SEQ_VIDEO_FORMATS};
use crate::frames::Frames;
use crate::icons::{self, Icon};
use crate::jobs::{Queue, Status};
use crate::player::{LoadState, MaxRes, Player};
use crate::settings::{self, Settings};
use crate::theme::{self, pal};
use crate::updater::{self, Updater};
use crate::util;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    Off,
    All,
    One,
}

/// A loaded file or stream and what VMPC found inside it.
struct Item {
    source: String,
    class: Option<Classified>,
}

impl Item {
    fn info(&self) -> Option<&MediaInfo> {
        match &self.class {
            Some(Classified::Media(info)) => Some(info),
            _ => None,
        }
    }
    fn kind(&self) -> Option<MediaKind> {
        self.info().map(|i| i.kind())
    }
    fn icon(&self) -> Icon {
        match &self.class {
            _ if util::is_url(&self.source) => Icon::Globe,
            Some(Classified::Media(i)) => kind_icon(i.kind()),
            Some(Classified::Subtitle(_)) => Icon::Playlist,
            Some(Classified::Archive(_)) => Icon::Archive,
            Some(Classified::Data(..) | Classified::Text(_) | Classified::Recognized) => Icon::Doc,
            _ => Icon::Folder,
        }
    }
}

fn kind_icon(kind: MediaKind) -> Icon {
    match kind {
        MediaKind::Video => Icon::Screen,
        MediaKind::Audio => Icon::Music,
        MediaKind::Image => Icon::Image,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Plain,
    Subtitle,
    Data(DataFormat),
}

/// A non-media file on the stage.
enum Doc {
    Text { source: String, text: String, lines: Vec<String>, kind: TextKind },
    Archive { source: String, entries: Vec<String> },
    Recognized { source: String },
    Unreadable { source: String, error: String },
}

impl Doc {
    fn new(source: String, class: Classified) -> Self {
        let text_doc = |source, text: String, kind| {
            let lines = text.lines().take(200_000).map(str::to_string).collect();
            Doc::Text { source, text, lines, kind }
        };
        match class {
            Classified::Subtitle(t) => text_doc(source, t, TextKind::Subtitle),
            Classified::Data(f, t) => text_doc(source, t, TextKind::Data(f)),
            Classified::Text(t) => text_doc(source, t, TextKind::Plain),
            Classified::Archive(entries) => Doc::Archive { source, entries },
            Classified::Unreadable(error) => Doc::Unreadable { source, error },
            Classified::Recognized | Classified::Media(_) | Classified::Playlist(_) => Doc::Recognized { source },
        }
    }
    fn source(&self) -> &str {
        match self {
            Doc::Text { source, .. } | Doc::Archive { source, .. } | Doc::Recognized { source } | Doc::Unreadable { source, .. } => source,
        }
    }
    fn subject(&self) -> Option<Subject> {
        match self {
            Doc::Text { kind: TextKind::Subtitle, .. } => Some(Subject::Subtitle),
            Doc::Text { kind: TextKind::Data(f), .. } => Some(Subject::Data(*f)),
            Doc::Archive { .. } => Some(Subject::Archive),
            _ => None,
        }
    }
    /// Short label for the header chip.
    fn label(&self) -> String {
        match self {
            Doc::Text { kind: TextKind::Subtitle, .. } => "SUBTITLES".into(),
            Doc::Text { kind: TextKind::Data(f), .. } => f.label().into(),
            Doc::Text { .. } => "TEXT".into(),
            Doc::Archive { entries, .. } => format!("ARCHIVE · {} files", entries.len()),
            Doc::Recognized { source } | Doc::Unreadable { source, .. } => {
                catalog::lookup(&catalog::ext_of(source)).first().map(|e| e.category.to_uppercase()).unwrap_or_else(|| "FILE".into())
            }
        }
    }
}

/// Options for subtitle / data / archive targets.
#[derive(Default)]
struct ExtraOpts {
    shift: f64,
    track: usize,
    out_dir: Option<PathBuf>,
}

/// What a home-page suggestion asked for, applied once the media is analysed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Intent {
    ToMp3,
    Gif,
    Frames,
    ImagesToVideo,
}

const SIDEBAR_W: f32 = 264.0;
const RAIL_W: f32 = 60.0;
const BAR_W: f32 = 390.0;

pub struct App {
    dark: bool,
    player: Player,
    library: Vec<Item>,
    current: Option<usize>,
    probe_tx: Sender<(String, Classified)>,
    probe_rx: Receiver<(String, Classified)>,
    /// The non-media file on the stage (text, archive, recognised file …).
    doc: Option<Doc>,
    /// The current item is still being analysed.
    awaiting: Option<String>,
    extra: ExtraOpts,
    archive_tx: Sender<Result<Vec<PathBuf>, String>>,
    archive_rx: Receiver<Result<Vec<PathBuf>, String>>,
    show_formats: bool,
    format_search: String,
    loop_mode: LoopMode,
    seek_drag: Option<f32>,
    url_input: String,
    converter: Converter,
    frames: Frames,
    target: Option<Target>,
    intent: Option<Intent>,
    convert_msg: Option<(String, bool)>,
    queue: Queue,
    updater: Updater,
    auto_update: bool,
    /// The user chose "Later" for this run.
    update_dismissed: bool,
    /// An update the user started failed: show why in the dialog.
    show_update_error: bool,
    greeting: &'static str,
    focus_composer: bool,
    toast: Option<(String, Instant)>,
    last_pointer_move: Instant,
    sidebar_open: bool,
    bar_open: bool,
    show_settings: bool,
    level: f32,
    title: String,
    frame: u64,
    revealed: Option<(String, Instant)>,
    saved: Settings,
    settings_dirty: Option<Instant>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<String>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let saved = settings::load();
        theme::apply(&cc.egui_ctx, saved.dark);
        let mut player = Player::default();
        player.volume = saved.volume.clamp(0.0, 1.0);
        player.muted = saved.muted;
        player.max_res = MaxRes::from_height(saved.max_height);
        player.hwaccel = saved.hwaccel;
        let (probe_tx, probe_rx) = mpsc::channel();
        let (archive_tx, archive_rx) = mpsc::channel();
        let updater = Updater::default();
        if saved.auto_update {
            // Silent: if there is no connection nothing is shown.
            updater.check(&cc.egui_ctx, false);
        }
        let mut app = Self {
            dark: saved.dark,
            player,
            library: Vec::new(),
            current: None,
            probe_tx,
            probe_rx,
            doc: None,
            awaiting: None,
            extra: ExtraOpts::default(),
            archive_tx,
            archive_rx,
            // VMPC_SHOW_FORMATS opens the format library at start-up (handy for screenshots).
            show_formats: std::env::var_os("VMPC_SHOW_FORMATS").is_some(),
            format_search: std::env::var("VMPC_SHOW_FORMATS").unwrap_or_default(),
            loop_mode: match saved.loop_mode {
                1 => LoopMode::All,
                2 => LoopMode::One,
                _ => LoopMode::Off,
            },
            seek_drag: None,
            url_input: String::new(),
            converter: Converter::default(),
            frames: Frames::default(),
            target: None,
            intent: None,
            convert_msg: None,
            queue: Queue::new(cc.egui_ctx.clone()),
            updater,
            auto_update: saved.auto_update,
            update_dismissed: false,
            show_update_error: false,
            greeting: greeting(),
            focus_composer: false,
            toast: None,
            last_pointer_move: Instant::now(),
            sidebar_open: true,
            bar_open: saved.tools_open,
            show_settings: false,
            level: 0.0,
            title: String::new(),
            frame: 0,
            revealed: None,
            saved,
            settings_dirty: None,
        };
        if !files.is_empty() {
            let files = expand(&files.iter().map(PathBuf::from).collect::<Vec<_>>());
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
                let _ = tx.send((src.clone(), catalog::classify(&src)));
                ctx.request_repaint();
            });
            self.library.push(Item { source, class: None });
        }
        if play_first && self.library.len() > first_new {
            self.play_index(first_new);
        }
    }

    fn play_index(&mut self, i: usize) {
        let Some(item) = self.library.get(i) else { return };
        self.current = Some(i);
        self.seek_drag = None;
        self.convert_msg = None;
        let (source, class) = (item.source.clone(), item.class.clone());
        match class {
            Some(class) => self.show_classified(source, class),
            None => {
                // Still analysing: shown as soon as the result arrives.
                self.player.stop();
                self.doc = None;
                self.awaiting = Some(source);
            }
        }
    }

    /// Puts an analysed item on the stage: media in the player, everything else as a document.
    fn show_classified(&mut self, source: String, class: Classified) {
        self.awaiting = None;
        self.extra.track = 0;
        match class {
            Classified::Media(info) => {
                self.doc = None;
                self.player.open_known(source, info);
            }
            other => {
                self.player.stop();
                self.doc = Some(Doc::new(source, other));
            }
        }
    }

    /// Handles a finished analysis; playlists are replaced by their entries.
    fn on_classified(&mut self, ctx: &egui::Context, source: String, class: Classified) {
        let Some(idx) = self.library.iter().position(|i| i.source == source) else { return };
        if let Classified::Playlist(entries) = class {
            let was_current = self.current == Some(idx);
            self.remove_item(idx);
            if was_current {
                self.awaiting = None;
            }
            let n = entries.len();
            self.toast(if n == 0 { "This playlist is empty".to_string() } else { format!("Playlist opened: {n} entries") });
            self.enqueue(ctx, entries, was_current || self.current.is_none());
            return;
        }
        self.library[idx].class = Some(class.clone());
        if self.awaiting.as_deref() == Some(source.as_str()) {
            self.show_classified(source, class);
        }
    }

    /// What the convert bar works on, and its source.
    fn subject(&self) -> Option<(Subject, String)> {
        if let Some((source, info)) = self.current_media() {
            let text_subtitles = info.subtitles.iter().filter(|c| !formats::is_bitmap_subtitle(c)).count();
            return Some((Subject::Media { kind: info.kind(), images: self.images().len(), text_subtitles }, source));
        }
        let doc = self.doc.as_ref()?;
        Some((doc.subject()?, doc.source().to_string()))
    }

    fn remove_item(&mut self, i: usize) {
        self.library.remove(i);
        match self.current {
            Some(c) if c == i => {
                self.current = None;
                self.go_home();
            }
            Some(c) if c > i => self.current = Some(c - 1),
            _ => {}
        }
    }

    fn clear_library(&mut self) {
        self.library.clear();
        self.current = None;
        self.target = None;
        self.go_home();
    }

    /// Back to the home page (nothing on stage).
    fn go_home(&mut self) {
        self.player.stop();
        self.doc = None;
        self.awaiting = None;
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

    fn open_url(&mut self, ctx: &egui::Context) {
        let url = self.url_input.trim().to_string();
        if !url.is_empty() {
            self.url_input.clear();
            self.enqueue(ctx, vec![url], true);
        }
    }

    /// The media in the preview, once analysed.
    fn current_media(&self) -> Option<(String, MediaInfo)> {
        Some((self.player.source.clone()?, self.player.info.clone()?))
    }

    fn images(&self) -> Vec<PathBuf> {
        self.library
            .iter()
            .filter(|i| i.kind() == Some(MediaKind::Image) && !util::is_url(&i.source))
            .map(|i| PathBuf::from(&i.source))
            .collect()
    }

    fn same_kind(&self, kind: MediaKind) -> Vec<(String, MediaInfo)> {
        self.library
            .iter()
            .filter_map(|i| i.info().filter(|info| info.kind() == kind).map(|info| (i.source.clone(), info.clone())))
            .collect()
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn snapshot(&mut self) {
        match self.player.snapshot(util::default_output_dir("Pictures")) {
            Some(p) => self.toast(format!("Snapshot saved to {}", p.display())),
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

    /// Keeps the selected conversion valid for the media in the preview and applies
    /// a pending home-page suggestion.
    fn sync_target(&mut self) {
        let Some((subject, _)) = self.subject() else { return };
        let kind = match subject {
            Subject::Media { kind, .. } => Some(kind),
            _ => None,
        };
        let groups = converter::groups_for_subject(subject);
        let fits = |t: Target| groups.iter().any(|g| g.targets.iter().any(|(x, _)| *x == t));
        if let Some(intent) = self.intent.take().filter(|_| kind.is_some()) {
            let kind = kind.unwrap();
            let wanted = match intent {
                Intent::ToMp3 => Target::Format { kind: 1, format: 0 },
                Intent::Gif if kind == MediaKind::Image => {
                    Target::ToVideo(SEQ_VIDEO_FORMATS.iter().position(|f| f.1 == "gif").unwrap_or(0))
                }
                Intent::Gif => {
                    let i = FORMATS.iter().filter(|f| f.kind == Kind::Video).position(|f| f.ext == "gif").unwrap_or(0);
                    Target::Format { kind: 0, format: i }
                }
                Intent::Frames => Target::Sequence(0),
                Intent::ImagesToVideo => Target::ToVideo(0),
            };
            if fits(wanted) {
                self.target = Some(wanted);
                self.bar_open = true;
            }
        }
        if !self.target.is_some_and(fits) {
            self.target = Some(converter::default_for_subject(subject));
        }
    }

    // ----- input --------------------------------------------------------------

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            let files = expand(&dropped);
            self.enqueue(ctx, files, true);
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
                Key::B if mods.command => self.sidebar_open = !self.sidebar_open,
                Key::Escape if self.show_settings => self.show_settings = false,
                Key::Escape if self.show_formats => self.show_formats = false,
                Key::Escape => ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false)),
                Key::T => self.bar_open = !self.bar_open,
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

    // ----- sidebar --------------------------------------------------------------------

    fn sidebar(&mut self, ctx: &egui::Context) {
        let p = pal();
        let frame = egui::Frame::none().fill(p.side).inner_margin(Margin { left: 12.0, right: 12.0, top: 12.0, bottom: 12.0 });
        let collapsed = egui::SidePanel::left("sidebar").resizable(false).exact_width(RAIL_W).frame(frame).show_separator_line(false);
        let expanded = egui::SidePanel::left("sidebar-full").resizable(false).exact_width(SIDEBAR_W).frame(frame).show_separator_line(false);
        egui::SidePanel::show_animated_between(ctx, self.sidebar_open, collapsed, expanded, |ui, t| {
            // Hairline on the sidebar's right edge.
            let r = ui.max_rect();
            ui.painter().vline(r.right() + 12.0, r.y_range().expand(12.0), Stroke::new(1.0_f32, p.border));
            if t < 0.5 {
                self.rail(ui);
            } else {
                ui.set_opacity(((t - 0.5) * 2.0).min(1.0));
                self.sidebar_full(ui);
            }
        });
    }

    fn rail(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.vertical_centered(|ui| {
            if theme::logo_button(ui, 34.0, false).on_hover_text("Open sidebar (Ctrl+B)").clicked() {
                self.sidebar_open = true;
            }
            ui.add_space(12.0);
            if theme::icon_button(ui, Icon::Upload, 36.0, false, "Open files (Ctrl+O)").clicked() {
                self.open_files(&ctx);
            }
            if theme::icon_button(ui, Icon::Folder, 36.0, false, "Open folder").clicked() {
                self.open_folder(&ctx);
            }
            if theme::icon_button(ui, Icon::Playlist, 36.0, false, "Library").clicked() {
                self.sidebar_open = true;
            }
            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                if theme::icon_button(ui, Icon::Gear, 36.0, self.show_settings, "Settings").clicked() {
                    self.show_settings = true;
                }
            });
        });
    }

    fn sidebar_full(&mut self, ui: &mut Ui) {
        let p = pal();
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            if theme::logo_button(ui, 34.0, true).on_hover_text("Close sidebar (Ctrl+B)").clicked() {
                self.sidebar_open = false;
            }
        });
        ui.add_space(10.0);
        if theme::menu_item(ui, Icon::Upload, "Open files", None, false).clicked() {
            self.open_files(&ctx);
        }
        if theme::menu_item(ui, Icon::Folder, "Open folder", None, false).clicked() {
            self.open_folder(&ctx);
        }
        if theme::menu_item(ui, Icon::Globe, "Open a link", None, false).clicked() {
            // Back to the home page, where the link goes into the composer.
            self.player.stop();
            self.current = None;
            self.focus_composer = true;
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            theme::section(ui, "Library");
            if !self.library.is_empty() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let clear = ui.add(egui::Label::new(RichText::new("Clear").size(12.0).color(p.muted)).sense(Sense::click()));
                    if clear.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.clear_library();
                    }
                });
            }
        });
        let bottom_h = 96.0;
        let list_h = (ui.available_height() - bottom_h).max(60.0);
        let mut play = None;
        let mut remove = None;
        egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            if self.library.is_empty() {
                theme::hint(ui, "Files you open appear here.");
            }
            for (i, item) in self.library.iter().enumerate() {
                let icon = item.icon();
                let resp = theme::menu_item(ui, icon, &util::display_name(&item.source), None, self.current == Some(i));
                let close = Rect::from_center_size(pos2(resp.rect.right() - 16.0, resp.rect.center().y), Vec2::splat(20.0));
                let close_resp = ui.interact(close, ui.id().with(("lib-x", i)), Sense::click());
                if resp.hovered() || close_resp.hovered() {
                    icons::paint(ui.painter(), close.shrink(5.0), Icon::Close, if close_resp.hovered() { p.danger } else { p.muted });
                }
                if close_resp.clicked() {
                    remove = Some(i);
                } else if resp.clicked() && self.current != Some(i) {
                    play = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            self.remove_item(i);
        } else if let Some(i) = play {
            self.play_index(i);
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            theme::hint(ui, &format!("VMPC {}", updater::current_version()));
            let known = catalog::unique_extensions();
            if theme::menu_item(ui, Icon::Doc, "Supported formats", Some(&format!("{known} known file types")), self.show_formats).clicked() {
                self.show_formats = true;
            }
            if theme::menu_item(ui, Icon::Gear, "Settings", None, self.show_settings).clicked() {
                self.show_settings = true;
            }
            let running = self.queue.snapshot().into_iter().find(|j| j.status == Status::Running);
            if let Some(job) = running {
                let label = format!("Converting {:.0}%", job.progress * 100.0);
                if theme::menu_item(ui, Icon::Convert, &label, Some(&job.title), false).clicked() {
                    self.bar_open = true;
                }
            }
        });
    }

    // ----- top bar ------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut Ui) {
        let p = pal();
        ui.horizontal(|ui| {
            ui.set_height(40.0);
            if let Some((source, info)) = self.current_media() {
                let t = theme::ease(ui.ctx(), Id::new(("title", &source)), self.frame > 2, 0.35);
                ui.set_opacity(t);
                ui.add(egui::Label::new(RichText::new(util::display_name(&source)).size(15.0).color(p.strong)).truncate())
                    .on_hover_text(info.summary());
                theme::chip(ui, info.kind().label(), p.accent);
                if let Some(v) = &info.video {
                    theme::chip(ui, &format!("{}×{}", v.width, v.height), p.muted);
                }
                if let Some(d) = info.duration.filter(|_| info.kind() != MediaKind::Image) {
                    theme::chip(ui, &util::fmt_time(d), p.muted);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.set_opacity(1.0);
                    let label = if self.bar_open { "Hide conversions" } else { "Convert" };
                    if theme::button_ex(ui, Some(Icon::Convert), label, !self.bar_open, true).clicked() {
                        self.bar_open = !self.bar_open;
                    }
                    if theme::icon_button(ui, Icon::Close, 34.0, false, "Back to home").clicked() {
                        self.go_home();
                        self.current = None;
                    }
                });
            } else if let Some(doc) = &self.doc {
                let source = doc.source().to_string();
                let (label, convertible) = (doc.label(), doc.subject().is_some());
                ui.add(egui::Label::new(RichText::new(util::display_name(&source)).size(15.0).color(p.strong)).truncate());
                theme::chip(ui, &label, p.accent);
                if let Ok(meta) = std::fs::metadata(&source) {
                    theme::chip(ui, &human_size(meta.len()), p.muted);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if convertible {
                        let label = if self.bar_open { "Hide conversions" } else { "Convert" };
                        if theme::button_ex(ui, Some(Icon::Convert), label, !self.bar_open, true).clicked() {
                            self.bar_open = !self.bar_open;
                        }
                    }
                    if theme::icon_button(ui, Icon::Upload, 34.0, false, "Open with the default app").clicked() {
                        util::open_with_default(std::path::Path::new(&source));
                    }
                    if theme::icon_button(ui, Icon::Close, 34.0, false, "Back to home").clicked() {
                        self.go_home();
                        self.current = None;
                    }
                });
            } else if let Some(source) = self.player.source.as_deref().or(self.awaiting.as_deref()) {
                ui.label(RichText::new(util::display_name(source)).size(15.0).color(p.muted));
            }
        });
    }

    // ----- home --------------------------------------------------------------------

    fn home(&mut self, ui: &mut Ui) {
        let p = pal();
        let ctx = ui.ctx().clone();
        let area = ui.available_rect_before_wrap();
        let width = (area.width() - 64.0).clamp(320.0, 680.0);
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let top = area.top() + (area.height() * 0.24).max(30.0);
        let column = Rect::from_min_size(pos2(area.center().x - width / 2.0, top), vec2(width, area.bottom() - top));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(column), |ui| {
            ui.vertical_centered(|ui| {
                // Greeting
                ui.horizontal(|ui| {
                    let greeting = self.greeting;
                    let g = ui.painter().layout_no_wrap(greeting.to_string(), theme::serif(34.0), p.strong);
                    let total = 44.0 + 12.0 + g.size().x;
                    ui.add_space(((width - total) / 2.0).max(0.0));
                    theme::logo_button(ui, 44.0, false);
                    ui.add_space(4.0);
                    ui.label(RichText::new(greeting).font(theme::serif(34.0)).color(p.strong));
                });
                ui.add_space(26.0);
                // Composer
                let (rect, resp) = ui.allocate_exact_size(vec2(width, 128.0), Sense::click());
                let hot = theme::ease(&ctx, Id::new("composer-hot"), hovering || resp.hovered(), 0.18);
                theme::soft_shadow(ui.painter(), rect, 18.0, 0.6 + 0.4 * hot);
                ui.painter().add(theme::surface_shape(rect, 18.0, p.surface, theme::lerp_color(p.border, p.accent, if hovering { 1.0 } else { 0.35 * hot })));
                let inner = rect.shrink2(vec2(18.0, 14.0));
                let mut submit = false;
                let mut edit_id = None;
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| {
                    let placeholder = if hovering { "Drop to open…" } else { "Paste a link to a stream, or drop files anywhere…" };
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.url_input)
                            .hint_text(RichText::new(placeholder).color(p.muted))
                            .frame(false)
                            .font(FontId::proportional(16.0))
                            .desired_width(inner.width()),
                    );
                    edit_id = Some(edit.id);
                    submit |= edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.add_space((inner.height() - 72.0).max(0.0));
                    ui.horizontal(|ui| {
                        if theme::button_icon(ui, Icon::Upload, "Open files", false).clicked() {
                            self.open_files(&ctx);
                        }
                        if theme::button_icon(ui, Icon::Folder, "Open folder", false).clicked() {
                            self.open_folder(&ctx);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let ready = !self.url_input.trim().is_empty();
                            let (r, send) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                            let color = if ready { p.accent } else { p.border };
                            ui.painter().rect_filled(r, 9.0, color);
                            icons::paint(ui.painter(), r.shrink(10.0), Icon::Play, Color32::WHITE);
                            submit |= ready && send.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open link").clicked();
                        });
                    });
                });
                if submit {
                    self.open_url(&ctx);
                }
                // Clicking anywhere in the composer focuses the link field, like a chat box.
                if resp.clicked() || std::mem::take(&mut self.focus_composer) {
                    if let Some(id) = edit_id {
                        ctx.memory_mut(|m| m.request_focus(id));
                    }
                }
                ui.add_space(18.0);
                // Suggestions
                let chips: [(Icon, &str, Option<Intent>, bool); 5] = [
                    (Icon::Play, "Play a video", None, false),
                    (Icon::Music, "Convert to MP3", Some(Intent::ToMp3), false),
                    (Icon::Image, "Make a GIF", Some(Intent::Gif), false),
                    (Icon::Frames, "Video → image sequence", Some(Intent::Frames), false),
                    (Icon::Screen, "Images → video", Some(Intent::ImagesToVideo), true),
                ];
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    for (icon, label, intent, folder) in chips {
                        if suggestion(ui, icon, label).clicked() {
                            self.intent = intent;
                            if folder {
                                self.open_folder(&ctx);
                            } else {
                                self.open_files(&ctx);
                            }
                        }
                    }
                });
                if ffmpeg::tools().is_none() {
                    ui.add_space(24.0);
                    engine_download_ui(ui);
                }
            });
        });
    }

    // ----- workspace -----------------------------------------------------------------

    fn workspace(&mut self, ctx: &egui::Context, fullscreen: bool, boot: f32) {
        let p = pal();
        let subject = self.subject();
        let active = !matches!(self.player.state, LoadState::Idle) || self.doc.is_some() || self.awaiting.is_some();
        let playable = self.player.is_loaded() && !self.player.is_still();
        let show_controls =
            playable && (!fullscreen || self.player.paused || self.last_pointer_move.elapsed() < Duration::from_millis(2500));

        // Slide-out conversions bar.
        egui::SidePanel::right("convert-bar")
            .resizable(false)
            .exact_width(BAR_W)
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::same(18.0)))
            .show_animated(ctx, !fullscreen && self.bar_open && subject.is_some(), |ui| {
                let r = ui.max_rect();
                ui.painter().vline(r.left() - 18.0, r.y_range().expand(18.0), Stroke::new(1.0_f32, p.border));
                if let Some((subject, source)) = &subject {
                    self.convert_bar(ui, *subject, source);
                }
            });

        let margin = if fullscreen { Margin::same(0.0) } else { Margin { left: 24.0, right: 24.0, top: 14.0, bottom: 18.0 } };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(if fullscreen { Color32::BLACK } else { p.bg }).inner_margin(margin))
            .show(ctx, |ui| {
                ui.set_opacity(boot);
                if !active {
                    self.home(ui);
                    return;
                }
                if !fullscreen {
                    self.top_bar(ui);
                    ui.add_space(8.0);
                }
                if self.doc.is_some() {
                    self.doc_view(ui);
                    return;
                }
                let controls_h = theme::ease(ctx, Id::new("controls"), show_controls, 0.25) * 104.0;
                let stage = Rect::from_min_max(ui.cursor().min, ui.max_rect().max - vec2(0.0, controls_h));
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(stage), |ui| self.stage(ui, fullscreen));
                if controls_h > 1.0 {
                    let bar = Rect::from_min_max(pos2(stage.left(), stage.bottom() + 12.0), ui.max_rect().max);
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(bar), |ui| {
                        ui.set_opacity(controls_h / 104.0);
                        self.controls(ui, ctx);
                    });
                }
            });
        if fullscreen && playable && !show_controls {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
    }

    fn convert_bar(&mut self, ui: &mut Ui, subject: Subject, source: &str) {
        let p = pal();
        let media = self.current_media();
        let images = self.images();
        let groups = converter::groups_for_subject(subject);
        let target = self.target.unwrap_or(converter::default_for_subject(subject));
        ui.horizontal(|ui| {
            theme::headline(ui, "Convert", 24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(ui, Icon::Close, 30.0, false, "Close (T)").clicked() {
                    self.bar_open = false;
                }
            });
        });
        let detected = match subject {
            Subject::Media { kind, .. } => kind.label().to_lowercase(),
            Subject::Subtitle => "subtitles".into(),
            Subject::Data(f) => format!("{} data", f.label()),
            Subject::Archive => "an archive".into(),
        };
        theme::hint(ui, &format!("Detected {detected} · showing everything that fits"));
        ui.add_space(6.0);
        let footer_h = 120.0;
        egui::ScrollArea::vertical().max_height(ui.available_height() - footer_h).auto_shrink([false, false]).show(ui, |ui| {
            for group in &groups {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
                    icons::paint(ui.painter(), r, group.icon, p.muted);
                    ui.label(RichText::new(&group.title).size(13.0).color(p.strong));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for (t, label) in &group.targets {
                        if converter::pill(ui, label, *t == target).clicked() {
                            self.target = Some(*t);
                            self.convert_msg = None;
                        }
                    }
                });
                // Options appear right under the group of the selected target.
                if group.targets.iter().any(|(t, _)| *t == target) {
                    ui.add_space(6.0);
                    let t = theme::ease(ui.ctx(), Id::new(("opts", format!("{target:?}"))), true, 0.22);
                    egui::Frame::none().fill(p.tint).rounding(12.0).inner_margin(12.0).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_opacity(t);
                        self.target_options(ui, target, media.as_ref(), &images);
                    });
                    ui.add_space(4.0);
                }
            }
            let jobs = self.queue.snapshot();
            if !jobs.is_empty() {
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(RichText::new("Queue").size(13.0).color(p.strong));
                converter::jobs_list(ui, &mut self.queue);
            }
        });
        // Sticky footer with the action.
        ui.add_space(8.0);
        let same = media.as_ref().map(|(_, i)| self.same_kind(i.kind())).unwrap_or_default();
        let (label, ready) = match target {
            Target::Format { .. } | Target::Custom => (self.converter.action_label(same.len()), self.converter.ready() && media.is_some()),
            Target::Sequence(_) => ("Extract frames".to_string(), media.as_ref().is_some_and(|m| m.1.duration.is_some())),
            Target::ToVideo(_) => ("Create video".to_string(), true),
            Target::Subtitle(_) if media.is_some() => ("Extract subtitles".to_string(), true),
            Target::Subtitle(_) => ("Convert subtitles".to_string(), true),
            Target::Data(f) => (format!("Convert to {}", f.label()), true),
            Target::Extract => ("Extract".to_string(), true),
            Target::Repack(_) => ("Repack".to_string(), true),
        };
        if theme::button_ex(ui, Some(Icon::Convert), &label, true, ready).clicked() {
            let result = self.start_target(target, source, media.as_ref(), &same, images);
            self.convert_msg = Some(match result {
                Ok(m) => (m, true),
                Err(e) => (e, false),
            });
        }
        if let Some((m, ok)) = &self.convert_msg {
            ui.add(egui::Label::new(RichText::new(m).size(12.5).color(if *ok { p.ok } else { p.danger })).truncate());
        }
    }

    /// Options shown under the selected target.
    fn target_options(&mut self, ui: &mut Ui, target: Target, media: Option<&(String, MediaInfo)>, images: &[PathBuf]) {
        let p = pal();
        match (target, media) {
            (Target::Format { .. } | Target::Custom, Some(m)) => {
                self.converter.set_target(target);
                let n = self.same_kind(m.1.kind()).len();
                self.converter.options_ui(ui, &m.1, n);
            }
            (Target::Sequence(i), Some(m)) => {
                self.frames.set_sequence_format(i);
                self.frames.extract_options_ui(ui, &m.1);
            }
            (Target::ToVideo(i), Some(m)) => {
                self.frames.set_video_format(i);
                let imgs = if images.is_empty() { vec![PathBuf::from(&m.0)] } else { images.to_vec() };
                self.frames.assemble_options_ui(ui, &imgs);
            }
            (Target::Subtitle(_), _) => {
                if let Some((_, info)) = media {
                    let tracks: Vec<(usize, &String)> =
                        info.subtitles.iter().enumerate().filter(|(_, c)| !formats::is_bitmap_subtitle(c)).collect();
                    if !tracks.iter().any(|(i, _)| *i == self.extra.track) {
                        self.extra.track = tracks.first().map(|t| t.0).unwrap_or(0);
                    }
                    if tracks.len() > 1 {
                        converter::field(ui, "Track", |ui| {
                            egui::ComboBox::from_id_salt("sub-track").selected_text(format!("Track {}", self.extra.track + 1)).show_ui(ui, |ui| {
                                for (i, codec) in &tracks {
                                    ui.selectable_value(&mut self.extra.track, *i, format!("Track {} · {codec}", i + 1));
                                }
                            });
                        });
                    }
                    if info.subtitles.iter().any(|c| formats::is_bitmap_subtitle(c)) {
                        theme::hint(ui, "Picture-based DVD/Blu-ray subtitle tracks can't be turned into text and are skipped.");
                    }
                }
                converter::field(ui, "Shift timing", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.extra.shift).range(-3600.0..=3600.0).speed(0.05).suffix(" s").max_decimals(2));
                        ui.label(RichText::new("positive = later").size(12.5).color(p.muted));
                    });
                });
                converter::save_to(ui, &mut self.extra.out_dir, "Same folder as the source");
            }
            (Target::Data(to), _) => {
                theme::hint(ui, &format!("Rewrites the data as {}. Tables become lists of records and back.", to.label()));
                converter::save_to(ui, &mut self.extra.out_dir, "Same folder as the source");
            }
            (Target::Extract, _) => {
                theme::hint(ui, "Unpacks every file into a new folder.");
                converter::save_to(ui, &mut self.extra.out_dir, "Next to the archive");
            }
            (Target::Repack(i), _) => {
                let (name, _) = crate::archive::REPACK_FORMATS[i];
                theme::hint(ui, &format!("Copies every file into a new {name} archive."));
                converter::save_to(ui, &mut self.extra.out_dir, "Next to the archive");
            }
            _ => {}
        }
    }

    /// Starts the selected conversion; returns a status line.
    fn start_target(
        &mut self,
        target: Target,
        source: &str,
        media: Option<&(String, MediaInfo)>,
        same: &[(String, MediaInfo)],
        images: Vec<PathBuf>,
    ) -> Result<String, String> {
        let dir = self.extra.out_dir.clone();
        let path = std::path::Path::new(source);
        match (target, media) {
            (Target::Format { .. } | Target::Custom, Some(m)) => Ok(self.converter.start(&mut self.queue, (&m.0, &m.1), same)),
            (Target::Sequence(_), Some(m)) => self.frames.start_extract(&mut self.queue, &m.0, &m.1),
            (Target::ToVideo(_), Some(m)) => {
                let imgs = if images.is_empty() { vec![PathBuf::from(&m.0)] } else { images };
                self.frames.start_assemble(&mut self.queue, &imgs)
            }
            (Target::Subtitle(i), _) => {
                let track = if media.is_some() { self.extra.track } else { 0 };
                let job = converter::subtitle_job(source, i, track, self.extra.shift, dir.as_deref());
                let msg = format!("Saving {}", job.output.display());
                self.queue.push(job);
                Ok(msg)
            }
            (Target::Data(to), _) => {
                let Some(Doc::Text { text, kind: TextKind::Data(from), .. }) = &self.doc else { return Err("No data file open".into()) };
                let out = crate::data::convert_file(path, text, *from, to, dir.as_deref())?;
                Ok(format!("Saved {}", out.display()))
            }
            (Target::Extract, _) => {
                let job = crate::archive::extract_job(path, dir.as_deref())?;
                let msg = format!("Extracting to {}", job.output.display());
                self.queue.push(job);
                Ok(msg)
            }
            (Target::Repack(i), _) => {
                let job = crate::archive::repack_job(path, crate::archive::REPACK_FORMATS[i].1, dir.as_deref())?;
                let msg = format!("Creating {}", job.output.display());
                self.queue.push(job);
                Ok(msg)
            }
            _ => Err("Nothing to convert".into()),
        }
    }

    // ----- documents (non-media files) ----------------------------------------------

    fn doc_view(&mut self, ui: &mut Ui) {
        let p = pal();
        let ctx = ui.ctx().clone();
        let rect = ui.available_rect_before_wrap();
        ui.painter().add(theme::surface_shape(rect, 14.0, p.surface, p.border));
        let inner = rect.shrink(18.0);
        let mut play_inside = None;
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| match self.doc.as_ref() {
            Some(Doc::Text { lines, text, kind, .. }) => {
                let note = match kind {
                    TextKind::Subtitle => "Subtitles · convert them with the Convert bar".to_string(),
                    TextKind::Data(f) => format!("{} data · convert it to another format with the Convert bar", f.label()),
                    TextKind::Plain => format!("{} lines", lines.len()),
                };
                theme::hint(ui, &note);
                if text.len() >= 2 * 1024 * 1024 - 16 {
                    theme::hint(ui, "Large file: showing the first 2 MB.");
                }
                ui.add_space(4.0);
                let row_h = 19.0;
                let digits = lines.len().max(1).to_string().len();
                egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, row_h, lines.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in range {
                        ui.horizontal(|ui| {
                            ui.set_height(row_h);
                            ui.label(RichText::new(format!("{:>digits$}", i + 1)).monospace().size(12.0).color(p.muted));
                            ui.add_space(6.0);
                            ui.add(egui::Label::new(RichText::new(&lines[i]).monospace().size(12.5).color(p.text)).extend());
                        });
                    }
                });
            }
            Some(Doc::Archive { source, entries }) => {
                let playable = entries.iter().filter(|e| matches!(catalog::handler_for_ext(&catalog::ext_of(e)), Handler::Media)).count();
                ui.horizontal(|ui| {
                    theme::headline(ui, &format!("{} files", entries.len()), 22.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if playable > 0 && theme::button_icon(ui, Icon::Play, &format!("Play {playable} media file{}", if playable == 1 { "" } else { "s" }), true).clicked() {
                            play_inside = Some(PathBuf::from(source));
                        }
                    });
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 26.0, entries.len(), |ui, range| {
                    for e in &entries[range] {
                        ui.horizontal(|ui| {
                            let handler = catalog::handler_for_ext(&catalog::ext_of(e));
                            let icon = match handler {
                                Handler::Media => Icon::Screen,
                                Handler::Archive => Icon::Archive,
                                Handler::Subtitle | Handler::Playlist => Icon::Playlist,
                                _ => Icon::Doc,
                            };
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
                            icons::paint(ui.painter(), r, icon, if handler == Handler::Media { p.accent } else { p.muted });
                            ui.add(egui::Label::new(RichText::new(e).size(13.0).color(p.text)).truncate());
                        });
                    }
                });
            }
            Some(Doc::Recognized { source }) | Some(Doc::Unreadable { source, .. }) => {
                let error = match self.doc.as_ref() {
                    Some(Doc::Unreadable { error, .. }) => Some(error.clone()),
                    _ => None,
                };
                recognized_card(ui, source, error.as_deref());
            }
            None => {}
        });
        if let Some(path) = play_inside {
            self.toast("Unpacking…");
            let (tx, ctx) = (self.archive_tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(crate::archive::extract_to_temp(&path).map(|files| {
                    files.into_iter().filter(|f| catalog::handler_for_ext(&catalog::ext_of(&f.to_string_lossy())) == Handler::Media).collect()
                }));
                ctx.request_repaint();
            });
        }
    }

    fn stage(&mut self, ui: &mut Ui, fullscreen: bool) {
        let p = pal();
        let ctx = ui.ctx().clone();
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click());
        let radius = if fullscreen { 0.0 } else { 14.0 };
        if !fullscreen {
            ui.painter().rect_filled(rect, radius, p.stage);
        }
        if resp.double_clicked() && self.player.is_loaded() {
            self.toggle_fullscreen(&ctx);
        }
        let dim_text = Color32::from_gray(170);
        match &self.player.state {
            LoadState::Idle => {
                if let Some(source) = &self.awaiting {
                    let c = rect.center();
                    ui.put(Rect::from_center_size(c, Vec2::splat(32.0)), egui::Spinner::new().size(32.0).color(p.accent));
                    let name = util::display_name(source);
                    ui.painter().text(c + vec2(0.0, 40.0), Align2::CENTER_CENTER, format!("Analysing {name}…"), FontId::proportional(14.0), dim_text);
                }
            }
            LoadState::Loading => {
                let c = rect.center();
                ui.put(Rect::from_center_size(c, Vec2::splat(32.0)), egui::Spinner::new().size(32.0).color(p.accent));
                let name = self.player.source.as_deref().map(util::display_name).unwrap_or_default();
                ui.painter().text(c + vec2(0.0, 40.0), Align2::CENTER_CENTER, format!("Opening {name}…"), FontId::proportional(14.0), dim_text);
            }
            LoadState::Failed(e) => {
                let e = e.clone();
                let c = rect.center();
                ui.painter().text(c - vec2(0.0, 30.0), Align2::CENTER_CENTER, "This file can't be opened", theme::serif(22.0), Color32::WHITE);
                ui.painter().text(c, Align2::CENTER_CENTER, theme::truncate(&e, 90), FontId::proportional(13.0), dim_text);
                let btn = Rect::from_center_size(c + vec2(0.0, 48.0), vec2(170.0, 36.0));
                if ui.put(btn, |ui: &mut Ui| theme::button_icon(ui, Icon::Folder, "Open another", true)).clicked() {
                    self.open_files(&ctx);
                }
            }
            LoadState::Ready => {
                let source = self.player.source.clone().unwrap_or_default();
                if let Some(tex) = &self.player.texture {
                    if self.revealed.as_ref().map(|(s, _)| s != &source).unwrap_or(true) {
                        self.revealed = Some((source.clone(), Instant::now()));
                    }
                    let elapsed = self.revealed.as_ref().map(|(_, t)| t.elapsed().as_secs_f32()).unwrap_or(1.0);
                    let k = egui::emath::easing::cubic_out((elapsed / 0.4).min(1.0));
                    if k < 1.0 {
                        ctx.request_repaint();
                    }
                    let size = tex.size_vec2();
                    let inner = if fullscreen { rect } else { rect.shrink(12.0) };
                    let scale = (inner.width() / size.x).min(inner.height() / size.y) * (0.97 + 0.03 * k);
                    let img = Rect::from_center_size(inner.center(), size * scale);
                    theme::rounded_image(ui.painter(), img, if fullscreen { 0.0 } else { 8.0 }, tex.id(), k);
                } else if !self.player.has_video() {
                    self.audio_visual(ui, rect);
                }
            }
        }
    }

    fn audio_visual(&mut self, ui: &mut Ui, rect: Rect) {
        let p = pal();
        let target = self.player.level().min(0.5) * 2.0;
        self.level += (target - self.level) * 0.25;
        let c = rect.center();
        let base = (rect.height() * 0.38).clamp(110.0, 260.0);
        for i in (1..=4).rev() {
            let r = base / 2.0 + i as f32 * (12.0 + 30.0 * self.level);
            let alpha = (0.08 + 0.22 * self.level) / i as f32;
            ui.painter().circle_stroke(c, r, Stroke::new(1.5_f32, p.accent.gamma_multiply(alpha)));
        }
        let disc = Rect::from_center_size(c, Vec2::splat(base));
        if let Some(cover) = &self.player.cover {
            theme::rounded_image(ui.painter(), disc, 16.0, cover.id(), 1.0);
        } else {
            icons::paint_logo_ex(ui.painter(), disc.shrink(base * 0.12), self.player.position() as f32 * 24.0);
        }
        if let Some(err) = self.player.audio_error() {
            ui.painter().text(pos2(c.x, disc.bottom() + 30.0), Align2::CENTER_CENTER, format!("Audio: {err}"), FontId::proportional(13.0), p.danger);
        }
    }

    fn controls(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let p = pal();
        let duration = self.player.duration();
        let pos = self.player.position();
        egui::Frame::none()
            .fill(p.surface)
            .stroke(Stroke::new(1.0_f32, p.border))
            .rounding(14.0)
            .inner_margin(Margin::symmetric(16.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let shown = match (self.seek_drag, duration) {
                        (Some(v), Some(d)) => v as f64 * d,
                        _ => pos,
                    };
                    ui.label(RichText::new(util::fmt_time(shown)).monospace().size(12.5).color(p.text));
                    let w = ui.available_width() - 56.0;
                    let mut v = match (self.seek_drag, duration) {
                        (Some(v), _) => v,
                        (None, Some(d)) if d > 0.0 => (pos / d) as f32,
                        _ => 0.0,
                    };
                    let resp = theme::slider(ui, &mut v, w, 4.0, None);
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
                    ui.label(RichText::new(dur_text).monospace().size(12.5).color(p.text));
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if theme::icon_button(ui, Icon::Prev, 36.0, false, "Previous (P)").clicked() {
                        self.prev();
                    }
                    let (icon, tip) = if self.player.paused { (Icon::Play, "Play (Space)") } else { (Icon::Pause, "Pause (Space)") };
                    if theme::hero_button(ui, icon, 44.0, tip).clicked() {
                        self.player.toggle_pause();
                    }
                    if theme::icon_button(ui, Icon::Next, 36.0, false, "Next (N)").clicked() {
                        self.next();
                    }
                    if self.player.has_video() && theme::icon_button(ui, Icon::Step, 36.0, false, "Next frame (E)").clicked() {
                        self.player.step_frame();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                        if theme::icon_button(ui, Icon::Fullscreen, 36.0, fs, "Fullscreen (F)").clicked() {
                            self.toggle_fullscreen(ctx);
                        }
                        let mut vol = self.player.volume;
                        if theme::slider(ui, &mut vol, 100.0, 4.0, None).changed() {
                            self.player.set_volume(vol);
                            if self.player.muted {
                                self.player.set_muted(false);
                            }
                        }
                        let vicon = if self.player.muted || self.player.volume == 0.0 { Icon::Mute } else { Icon::Volume };
                        if theme::icon_button(ui, vicon, 36.0, self.player.muted, "Mute (M)").clicked() {
                            self.player.set_muted(!self.player.muted);
                        }
                        if self.player.has_video() && theme::icon_button(ui, Icon::Camera, 36.0, false, "Snapshot (S)").clicked() {
                            self.snapshot();
                        }
                        let speed = self.player.speed;
                        egui::ComboBox::from_id_salt("speed").width(60.0).selected_text(format!("{speed}×")).show_ui(ui, |ui| {
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
                        let r = theme::icon_button(ui, Icon::Loop, 36.0, self.loop_mode != LoopMode::Off, loop_tip);
                        if self.loop_mode == LoopMode::One {
                            ui.painter().text(r.rect.right_top() + vec2(-7.0, 7.0), Align2::CENTER_CENTER, "1", FontId::proportional(10.0), p.accent);
                        }
                        if r.clicked() {
                            self.cycle_loop();
                        }
                    });
                });
            });
    }

    // ----- dialogs ------------------------------------------------------------------

    /// Centred dialog over a dimmed backdrop. Returns true if the backdrop was clicked.
    fn modal(ctx: &egui::Context, id: &str, width: f32, add: impl FnOnce(&mut Ui)) -> bool {
        let p = pal();
        let screen = ctx.screen_rect();
        let t = theme::ease(ctx, Id::new((id, "fade")), true, 0.18);
        let mut dismissed = false;
        egui::Area::new(Id::new(id)).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let backdrop = ui.allocate_rect(screen, Sense::click());
            ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha((90.0 * t) as u8));
            dismissed = backdrop.clicked();
            let height = (screen.height() - 80.0).min(640.0);
            let card = Rect::from_center_size(screen.center() + vec2(0.0, 8.0 * (1.0 - t)), vec2(width.min(screen.width() - 40.0), height));
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card), |ui| {
                ui.set_opacity(t);
                let frame = egui::Frame::none()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .rounding(16.0)
                    .shadow(egui::epaint::Shadow { offset: vec2(0.0, 12.0), blur: 40.0, spread: 0.0, color: p.shadow })
                    .inner_margin(24.0);
                frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui);
                });
            });
        });
        dismissed
    }

    fn update_dialog(&mut self, ctx: &egui::Context) {
        let state = self.updater.state();
        let visible = match &state {
            updater::State::Available(_) => !self.update_dismissed,
            updater::State::Downloading { .. } | updater::State::Installing => true,
            updater::State::Failed(_) => !self.update_dismissed && self.frame > 0 && self.show_update_error,
            _ => false,
        };
        if !visible {
            return;
        }
        let p = pal();
        let mut later = false;
        let mut start = None;
        let outside = Self::modal(ctx, "update-dialog", 460.0, |ui| {
            ui.horizontal(|ui| {
                theme::logo_button(ui, 38.0, false);
                ui.add_space(4.0);
                theme::headline(ui, "Update available", 24.0);
            });
            ui.add_space(6.0);
            match &state {
                updater::State::Available(r) => {
                    ui.label(
                        RichText::new(format!("VMPC {} is ready. You have {}.", r.version, updater::current_version())).color(p.text),
                    );
                    let notes = updater::whats_new(&r.notes);
                    if !notes.is_empty() {
                        ui.add_space(6.0);
                        theme::well(ui, |ui| {
                            ui.set_width(ui.available_width());
                            egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                                for (heading, line) in notes {
                                    let text = RichText::new(line).size(if heading { 13.5 } else { 12.5 });
                                    ui.label(if heading { text.color(p.strong) } else { text.color(p.text) });
                                }
                            });
                        });
                    }
                    ui.add_space(4.0);
                    let how = match updater::install_kind() {
                        updater::InstallKind::Manual => "This copy wasn't installed with the installer, so the download page will open.",
                        _ => "VMPC downloads the update, checks it and restarts by itself.",
                    };
                    theme::hint(ui, how);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if theme::button_icon(ui, Icon::Upload, "Update now", true).clicked() {
                                start = Some(r.clone());
                            }
                            if theme::button(ui, "Later", false).clicked() {
                                later = true;
                            }
                        });
                    });
                }
                updater::State::Downloading { release, progress } => {
                    ui.label(RichText::new(format!("Downloading VMPC {}…", release.version)).color(p.text));
                    ui.add_space(6.0);
                    theme::progress(ui, *progress, ui.available_width(), false);
                    theme::hint(ui, &format!("{:.0}%", progress * 100.0));
                }
                updater::State::Installing => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().color(p.accent));
                        ui.label(RichText::new("Installing… VMPC will restart in a moment.").color(p.text));
                    });
                }
                updater::State::Failed(e) => {
                    ui.label(RichText::new(e).color(p.danger));
                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::button(ui, "Close", false).clicked() {
                            later = true;
                        }
                    });
                }
                _ => {}
            }
        });
        let busy = matches!(state, updater::State::Downloading { .. } | updater::State::Installing);
        if (later || outside) && !busy {
            self.update_dismissed = true;
            self.show_update_error = false;
        }
        if let Some(release) = start {
            self.show_update_error = true;
            self.updater.install(ctx, release);
        }
    }

    /// Searchable library of every known format and what VMPC does with it.
    fn formats_dialog(&mut self, ctx: &egui::Context) {
        let p = pal();
        let rows = catalog::with_handlers();
        let mut close = false;
        let query = self.format_search.trim().to_lowercase();
        let filtered: Vec<&(catalog::Entry, Handler)> = rows
            .iter()
            .filter(|(e, h)| {
                query.is_empty()
                    || e.ext.contains(query.trim_start_matches('.'))
                    || e.desc.to_lowercase().contains(&query)
                    || e.category.to_lowercase().contains(&query)
                    || h.label().to_lowercase().contains(&query)
            })
            .collect();
        let count = |f: fn(&Handler) -> bool| {
            let mut exts: Vec<&str> = rows.iter().filter(|(_, h)| f(h)).map(|(e, _)| e.ext).collect();
            exts.sort_unstable();
            exts.dedup();
            exts.len()
        };
        let total = count(|_| true);
        let opens = count(|h| !matches!(h, Handler::Recognized | Handler::Unknown));
        let converts = count(|h| matches!(h, Handler::Media | Handler::Subtitle | Handler::Data(_) | Handler::Archive));
        let outside = Self::modal(ctx, "formats-dialog", 760.0, |ui| {
            ui.horizontal(|ui| {
                theme::headline(ui, "Supported formats", 26.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(ui, Icon::Close, 32.0, false, "Close (Esc)").clicked() {
                        close = true;
                    }
                });
            });
            theme::hint(
                ui,
                &format!(
                    "{total} file types known · {opens} open in VMPC · {converts} can be converted · the rest are recognised by name. \
                     Catalogue from Wikipedia's List of file formats (CC BY-SA 4.0)."
                ),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.format_search)
                    .hint_text("Search: an extension (mkv), a name (photoshop) or a category (archive)…")
                    .desired_width(ui.available_width())
                    .margin(vec2(10.0, 7.0)),
            );
            ui.add_space(6.0);
            theme::hint(ui, &format!("{} result{}", filtered.len(), if filtered.len() == 1 { "" } else { "s" }));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 40.0, filtered.len(), |ui, range| {
                for (e, h) in &filtered[range] {
                    ui.horizontal(|ui| {
                        ui.set_height(36.0);
                        ui.add_sized(vec2(70.0, 20.0), egui::Label::new(RichText::new(format!(".{}", e.ext)).monospace().strong().color(p.strong)));
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width() - 170.0);
                            ui.add(egui::Label::new(RichText::new(e.desc).size(13.0).color(p.text)).truncate());
                            let place = if e.section == e.category { e.category.to_string() } else { format!("{} · {}", e.category, e.section) };
                            ui.add(egui::Label::new(RichText::new(place).size(11.5).color(p.muted)).truncate());
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let color = match h {
                                Handler::Media => p.accent,
                                Handler::Recognized | Handler::Unknown => p.muted,
                                _ => p.ok,
                            };
                            theme::chip(ui, h.label(), color);
                        });
                    });
                }
            });
        });
        if close || outside {
            self.show_formats = false;
        }
    }

    fn settings_dialog(&mut self, ctx: &egui::Context) {
        let p = pal();
        let mut close = false;
        let outside = Self::modal(ctx, "settings-dialog", 480.0, |ui| {
            ui.horizontal(|ui| {
                theme::headline(ui, "Settings", 26.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(ui, Icon::Close, 32.0, false, "Close (Esc)").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                theme::section(ui, "APPEARANCE");
                let mut theme_idx = usize::from(self.dark);
                if theme::segmented(ui, &mut theme_idx, &[(Icon::Sun, "Light"), (Icon::Moon, "Dark")]) {
                    self.dark = theme_idx == 1;
                    theme::apply(ctx, self.dark);
                }
                ui.add_space(14.0);

                theme::section(ui, "UPDATES");
                theme::toggle(ui, &mut self.auto_update, "Check for updates when VMPC starts");
                ui.horizontal(|ui| {
                    let state = self.updater.state();
                    let busy = matches!(state, updater::State::Checking | updater::State::Downloading { .. } | updater::State::Installing);
                    if theme::button_ex(ui, None, "Check now", false, !busy).clicked() {
                        self.update_dismissed = false;
                        self.updater.check(ctx, true);
                    }
                    let (text, color) = match state {
                        updater::State::Checking => ("Checking…".to_string(), p.muted),
                        updater::State::UpToDate => (format!("You're up to date ({})", updater::current_version()), p.ok),
                        updater::State::Available(r) => (format!("VMPC {} is available", r.version), p.accent),
                        updater::State::Failed(e) => (e, p.muted),
                        _ => (format!("Version {}", updater::current_version()), p.muted),
                    };
                    ui.label(RichText::new(text).size(12.5).color(color));
                });
                theme::hint(ui, "Without an internet connection VMPC simply skips the check.");
                ui.add_space(14.0);

                theme::section(ui, "PLAYBACK");
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
                ui.add_space(14.0);

                theme::section(ui, "ENGINE");
                match ffmpeg::tools() {
                    Some(t) => {
                        ui.label(RichText::new(&t.version).size(13.0).color(p.text));
                        theme::hint(ui, &t.ffmpeg.display().to_string());
                    }
                    None => engine_download_ui(ui),
                }
                ui.add_space(14.0);

                theme::section(ui, "SHORTCUTS");
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
                        ("T", "Show / hide conversions"),
                        ("Ctrl+O", "Open files"),
                        ("Ctrl+B", "Sidebar"),
                    ] {
                        ui.label(RichText::new(k).monospace().color(p.accent));
                        ui.label(RichText::new(d).color(p.text));
                        ui.end_row();
                    }
                });
                ui.add_space(14.0);

                theme::section(ui, "ABOUT");
                ui.label(RichText::new(format!("VMPC {}", updater::current_version())).color(p.strong));
                theme::hint(
                    ui,
                    "Uses FFmpeg (ffmpeg.org) as its codec engine, run as a separate program; FFmpeg is licensed under \
                     the GNU GPL/LGPL. Fonts: Inter and Source Serif 4 (SIL Open Font License). See THIRD_PARTY_NOTICES.md.",
                );
            });
        });
        if close || outside {
            self.show_settings = false;
        }
    }

    // ----- overlays -------------------------------------------------------------------

    fn overlays(&mut self, ctx: &egui::Context) {
        let p = pal();
        let screen = ctx.screen_rect();
        let active = !matches!(self.player.state, LoadState::Idle);
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        // On the home page the composer itself reacts; elsewhere show an overlay.
        let t = theme::ease(ctx, Id::new("drop-overlay"), hovering && active, 0.18);
        if t > 0.01 {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("drop-overlay")));
            painter.rect_filled(screen, 0.0, p.bg.gamma_multiply(0.85 * t));
            let card = Rect::from_center_size(screen.center(), vec2(320.0, 140.0) * (0.96 + 0.04 * t));
            theme::soft_shadow(&painter, card, 16.0, t);
            painter.add(theme::surface_shape(card, 16.0, p.surface.gamma_multiply(t), p.accent.gamma_multiply(t)));
            icons::paint(&painter, Rect::from_center_size(card.center() - vec2(0.0, 18.0), Vec2::splat(30.0)), Icon::Upload, p.accent.gamma_multiply(t));
            painter.text(card.center() + vec2(0.0, 30.0), Align2::CENTER_CENTER, "Drop to open", theme::serif(20.0), p.strong.gamma_multiply(t));
        }
        if let Some((text, at)) = &self.toast {
            let age = at.elapsed().as_secs_f32();
            if age > 3.5 {
                self.toast = None;
            } else {
                let fade = (age / 0.2).min(1.0).min((3.5 - age) / 0.3);
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("toast")));
                let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(13.5), p.strong.gamma_multiply(fade));
                let size = galley.size() + vec2(28.0, 18.0);
                let rect = Rect::from_center_size(pos2(screen.center().x, screen.bottom() - 60.0 + 8.0 * (1.0 - fade)), size);
                theme::soft_shadow(&painter, rect, 10.0, fade);
                painter.add(theme::surface_shape(rect, 10.0, p.surface.gamma_multiply(fade), p.border.gamma_multiply(fade)));
                painter.galley(rect.min + vec2(14.0, 9.0), galley, p.strong);
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
            tools_open: self.bar_open,
            auto_update: self.auto_update,
        }
    }
}

/// Time-of-day greeting, like Claude's home screen (computed once at start-up).
fn greeting() -> &'static str {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    match local_hour(secs) {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    }
}

#[cfg(windows)]
fn local_hour(_utc_secs: u64) -> u32 {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(t: *mut SystemTime);
    }
    let mut t = SystemTime { year: 0, month: 0, day_of_week: 0, day: 0, hour: 0, minute: 0, second: 0, millis: 0 };
    // SAFETY: GetLocalTime only writes into the provided, correctly sized struct.
    unsafe { GetLocalTime(&mut t) };
    t.hour as u32
}

#[cfg(not(windows))]
fn local_hour(utc_secs: u64) -> u32 {
    // `date +%H` honours the user's time zone without pulling in a time crate.
    std::process::Command::new("date")
        .arg("+%H")
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(((utc_secs / 3600) % 24) as u32)
}

fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.1} KB", b / 1024.0),
        1_048_576..=1_073_741_823 => format!("{:.1} MB", b / 1_048_576.0),
        _ => format!("{:.2} GB", b / 1_073_741_824.0),
    }
}

/// Card for a file VMPC recognises but doesn't open itself (or couldn't read).
fn recognized_card(ui: &mut Ui, source: &str, error: Option<&str>) {
    let p = pal();
    let ext = catalog::ext_of(source);
    let entries = catalog::lookup(&ext);
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.16).max(10.0));
        let (r, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
        icons::paint(ui.painter(), r, Icon::Doc, p.accent);
        ui.add_space(8.0);
        let title = if ext.is_empty() { "File".to_string() } else { format!(".{}", ext.to_uppercase()) };
        theme::headline(ui, &title, 34.0);
        match entries.first() {
            Some(first) => {
                ui.label(RichText::new(first.desc).size(15.0).color(p.text));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let mut seen = Vec::new();
                    let width: f32 = entries.iter().map(|e| e.category.len() as f32 * 7.0 + 24.0).sum();
                    ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                    for e in &entries {
                        if !seen.contains(&e.category) {
                            seen.push(e.category);
                            theme::chip(ui, e.category, p.muted);
                        }
                    }
                });
                for e in entries.iter().skip(1).take(3) {
                    theme::hint(ui, &format!("Also: {}", e.desc));
                }
            }
            None => {
                ui.label(RichText::new("VMPC doesn't know this file type.").size(15.0).color(p.text));
            }
        }
        ui.add_space(10.0);
        let note = match (error, entries.is_empty()) {
            (Some(e), _) => format!("VMPC couldn't open it: {}", theme::truncate(e, 120)),
            (None, true) => format!(
                "It isn't among the {} formats VMPC knows, and it's neither media nor text.",
                catalog::unique_extensions()
            ),
            (None, false) => "VMPC recognises this format. It plays and converts media, subtitles, playlists, archives and text; \
                              for this one, use the app that made it."
                .to_string(),
        };
        ui.label(RichText::new(note).size(13.0).color(if error.is_some() { p.danger } else { p.muted }));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 330.0).max(0.0) / 2.0);
            if theme::button_icon(ui, Icon::Upload, "Open with default app", true).clicked() {
                util::open_with_default(std::path::Path::new(source));
            }
            if theme::button_icon(ui, Icon::Folder, "Show in folder", false).clicked() {
                util::reveal(std::path::Path::new(source));
            }
        });
    });
}

/// Home-page suggestion chip.
fn suggestion(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let p = pal();
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(40.0, 16.0), Sense::click());
    let hover = theme::ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.12);
    ui.painter().add(theme::surface_shape(rect, 10.0, theme::lerp_color(p.bg, p.surface, 0.5 + 0.5 * hover), p.border));
    icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), Vec2::splat(14.0)), icon, theme::lerp_color(p.muted, p.accent, hover));
    ui.painter().galley(pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0), galley, theme::lerp_color(p.text, p.strong, hover));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
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
            ui.label(RichText::new("The FFmpeg engine is missing; VMPC needs it to play and convert.").color(p.danger));
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
    /// eframe clears to near-black by default; use the canvas colour instead.
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
        while let Ok((src, class)) = self.probe_rx.try_recv() {
            self.on_classified(ctx, src, class);
        }
        while let Ok(result) = self.archive_rx.try_recv() {
            match result {
                Ok(files) if files.is_empty() => self.toast("No playable media inside"),
                Ok(files) => {
                    let files = files.into_iter().map(|f| f.to_string_lossy().into_owned()).collect();
                    self.enqueue(ctx, files, true);
                }
                Err(e) => self.toast(e),
            }
        }
        self.handle_drops(ctx);
        self.handle_keys(ctx);
        let repaint = self.player.update(ctx);
        self.sync_target();
        if self.player.take_ended_event() {
            let has_next = self.current.is_some_and(|i| i + 1 < self.library.len());
            match self.loop_mode {
                LoopMode::One => self.player.set_paused(false),
                LoopMode::All if self.library.len() <= 1 => self.player.set_paused(false),
                LoopMode::All => self.next(),
                LoopMode::Off if has_next => self.next(),
                LoopMode::Off => {}
            }
        }
        let title = match self.player.source.as_deref() {
            Some(s) => format!("{} — VMPC", util::display_name(s)),
            None => "VMPC".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        let boot = theme::ease(ctx, Id::new("boot"), self.frame > 1, 0.5);
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if !fullscreen {
            self.sidebar(ctx);
        }
        self.workspace(ctx, fullscreen, boot);
        if self.show_settings {
            self.settings_dialog(ctx);
        }
        if self.show_formats {
            self.formats_dialog(ctx);
        }
        self.update_dialog(ctx);
        self.overlays(ctx);

        if let Some(d) = repaint {
            ctx.request_repaint_after(d);
        }
        if self.queue.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if fullscreen && !self.player.paused {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}
