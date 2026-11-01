//! "Frames" tab: video → image sequence (like `ffmpeg -vf fps=N out_%06d.png`) and
//! the way back, image sequence → video, with optional reversed order.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use egui::{vec2, RichText, Ui};

use crate::converter::{jobs_card, pill};
use crate::ffmpeg::{self, MediaInfo};
use crate::formats::{self, Res, VCodec, IMAGE_SEQ_FORMATS, SEQ_VIDEO_FORMATS};
use crate::icons::Icon;
use crate::jobs::{Job, Post, Queue};
use crate::theme::{self, pal};
use crate::util;

pub struct Frames {
    mode: usize,
    // Video → images
    video: Option<String>,
    video_info: Option<Result<MediaInfo, String>>,
    probe_rx: Option<Receiver<Result<MediaInfo, String>>>,
    thumb: Option<egui::TextureHandle>,
    thumb_rx: Option<Receiver<Option<egui::ColorImage>>>,
    original_fps: bool,
    fps: f64,
    img_format: usize,
    quality: f32,
    res: Res,
    start: String,
    end: String,
    reverse_out: bool,
    prefix: String,
    out_dir: Option<PathBuf>,
    // Images → video
    images: Vec<PathBuf>,
    seq_fps: f64,
    seq_format: usize,
    seq_quality: f32,
    seq_reverse: bool,
    seq_boomerang: bool,
    seq_output: Option<PathBuf>,
    message: Option<(String, bool)>,
}

impl Default for Frames {
    fn default() -> Self {
        Self {
            mode: 0,
            video: None,
            video_info: None,
            probe_rx: None,
            thumb: None,
            thumb_rx: None,
            original_fps: false,
            fps: 10.0,
            img_format: 0,
            quality: 90.0,
            res: Res::Keep,
            start: String::new(),
            end: String::new(),
            reverse_out: false,
            prefix: "frame_".into(),
            out_dir: None,
            images: Vec::new(),
            seq_fps: 24.0,
            seq_format: 0,
            seq_quality: 75.0,
            seq_reverse: false,
            seq_boomerang: false,
            seq_output: None,
            message: None,
        }
    }
}

const MODES: [(Icon, &str); 2] = [(Icon::Frames, "Video → Images"), (Icon::Screen, "Images → Video")];

impl Frames {
    /// Routes dropped/opened paths to the right mode.
    pub fn add_paths(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let mut imgs: Vec<PathBuf> = Vec::new();
        for p in &paths {
            if p.is_dir() {
                imgs.extend(list_images(p));
            } else if util::is_image_path(p) {
                imgs.push(p.clone());
            }
        }
        if !imgs.is_empty() && (paths.len() > 1 || paths[0].is_dir()) {
            self.mode = 1;
            self.set_images(imgs);
        } else if let Some(first) = paths.first() {
            self.mode = 0;
            self.set_video(first.to_string_lossy().into_owned(), ctx);
        }
    }

    fn set_images(&mut self, mut imgs: Vec<PathBuf>) {
        imgs.sort_by(|a, b| util::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
        imgs.dedup();
        self.images = imgs;
        self.seq_output = None;
    }

    pub fn set_video(&mut self, source: String, ctx: &egui::Context) {
        self.video = Some(source.clone());
        self.video_info = None;
        self.thumb = None;
        self.out_dir = None;
        let (tx, rx) = mpsc::channel();
        let (ttx, trx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let info = ffmpeg::probe(&source);
            let thumb = info.as_ref().ok().and_then(|i| i.video.as_ref()).and_then(|v| {
                let (w, h) = ffmpeg::fit_size(v.width, v.height, 480, 270);
                let at = i_duration(&info).map(|d| (d * 0.1).min(5.0)).unwrap_or(0.0);
                ffmpeg::grab_frame(&source, at, false, w, h)
            });
            let _ = tx.send(info);
            let _ = ttx.send(thumb);
            ctx.request_repaint();
        });
        self.probe_rx = Some(rx);
        self.thumb_rx = Some(trx);
    }

    pub fn ui(&mut self, ui: &mut Ui, queue: &mut Queue) {
        if let Some(rx) = &self.probe_rx {
            if let Ok(info) = rx.try_recv() {
                if let Ok(i) = &info {
                    if let Some(v) = &i.video {
                        if self.original_fps {
                            self.fps = v.fps;
                        }
                    }
                }
                self.video_info = Some(info);
                self.probe_rx = None;
            }
        }
        if let Some(rx) = &self.thumb_rx {
            if let Ok(img) = rx.try_recv() {
                self.thumb = img.map(|i| ui.ctx().load_texture("frames-thumb", i, egui::TextureOptions::LINEAR));
                self.thumb_rx = None;
            }
        }
        let wide = ui.available_width() > 900.0;
        let col_w = if wide { (ui.available_width() - 30.0) / 2.0 } else { ui.available_width() - 8.0 };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                if theme::segmented(ui, &mut self.mode, &MODES) {
                    self.message = None;
                }
            });
            ui.add_space(12.0);
            let left = |s: &mut Self, ui: &mut Ui| {
                if s.mode == 0 {
                    s.video_source_card(ui, col_w);
                } else {
                    s.images_source_card(ui, col_w);
                }
            };
            if wide {
                ui.horizontal_top(|ui| {
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        left(self, ui);
                        ui.add_space(14.0);
                        jobs_card(ui, queue, col_w);
                    });
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        if self.mode == 0 {
                            self.extract_settings(ui, queue, col_w);
                        } else {
                            self.assemble_settings(ui, queue, col_w);
                        }
                    });
                });
            } else {
                left(self, ui);
                ui.add_space(14.0);
                if self.mode == 0 {
                    self.extract_settings(ui, queue, col_w);
                } else {
                    self.assemble_settings(ui, queue, col_w);
                }
                ui.add_space(14.0);
                jobs_card(ui, queue, col_w);
            }
            ui.add_space(20.0);
        });
    }

    // ----- Video → images -------------------------------------------------

    fn video_source_card(&mut self, ui: &mut Ui, w: f32) {
        let p = pal();
        theme::card(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.horizontal(|ui| {
                theme::section_title(ui, "Source video");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.video.is_some() && theme::button_icon(ui, Icon::Folder, "Change", false).clicked() {
                        if let Some(f) = rfd::FileDialog::new().pick_file() {
                            self.set_video(f.to_string_lossy().into_owned(), ui.ctx());
                        }
                    }
                });
            });
            match &self.video {
                None => {
                    let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
                    if theme::drop_zone(
                        ui,
                        vec2(ui.available_width(), 240.0),
                        "Drop a video here",
                        "any format · click to browse",
                        hovering,
                    )
                    .clicked()
                    {
                        if let Some(f) = rfd::FileDialog::new().pick_file() {
                            self.set_video(f.to_string_lossy().into_owned(), ui.ctx());
                        }
                    }
                }
                Some(src) => {
                    ui.label(RichText::new(util::display_name(src)).color(p.strong));
                    match &self.video_info {
                        None => theme::hint(ui, "Analysing…"),
                        Some(Err(e)) => {
                            ui.label(RichText::new(e).color(p.danger));
                        }
                        Some(Ok(info)) => {
                            theme::hint(ui, &format!("{}  ·  {}", util::fmt_time(info.duration.unwrap_or(0.0)), info.summary()));
                        }
                    }
                    if let Some(t) = &self.thumb {
                        ui.add_space(6.0);
                        let size = t.size_vec2();
                        let scale = ((ui.available_width()) / size.x).min(1.0);
                        let (rect, _) = ui.allocate_exact_size(size * scale, egui::Sense::hover());
                        theme::paint_raised(ui.painter(), rect, 12.0, 5.0);
                        theme::rounded_image(ui.painter(), rect.shrink(4.0), 9.0, t.id());
                    }
                }
            }
        });
    }

    fn range(&self) -> (f64, Option<f64>) {
        let start = util::parse_time(&self.start).unwrap_or(0.0);
        (start, util::parse_time(&self.end).filter(|e| *e > start))
    }

    fn extract_settings(&mut self, ui: &mut Ui, queue: &mut Queue, w: f32) {
        let p = pal();
        let info = match &self.video_info {
            Some(Ok(i)) if i.video.is_some() => Some(i.clone()),
            _ => None,
        };
        let src_fps = info.as_ref().and_then(|i| i.video.as_ref()).map(|v| v.fps);
        theme::card(ui, |ui| {
            ui.set_width(w - 32.0);
            theme::section_title(ui, "Image sequence settings");
            ui.add_space(4.0);
            egui::Grid::new("extract-grid").num_columns(2).spacing(vec2(16.0, 12.0)).show(ui, |ui| {
                ui.label("Frames / second");
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let r = theme::toggle(ui, &mut self.original_fps, "Every frame (original)");
                        if r.changed() && self.original_fps {
                            if let Some(f) = src_fps {
                                self.fps = f;
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_enabled(
                            !self.original_fps,
                            egui::DragValue::new(&mut self.fps).range(0.01..=240.0).speed(0.1).suffix(" fps").max_decimals(3),
                        );
                        if !self.original_fps {
                            for v in [1.0, 5.0, 10.0, 24.0, 30.0, 60.0] {
                                if pill(ui, &format!("{v}"), (self.fps - v).abs() < 1e-6).clicked() {
                                    self.fps = v;
                                }
                            }
                        }
                    });
                });
                ui.end_row();

                ui.label("Image format");
                ui.horizontal_wrapped(|ui| {
                    for (i, (name, _, _)) in IMAGE_SEQ_FORMATS.iter().enumerate() {
                        if pill(ui, name, self.img_format == i).clicked() {
                            self.img_format = i;
                        }
                    }
                });
                ui.end_row();

                let codec = IMAGE_SEQ_FORMATS[self.img_format].2;
                ui.label("Quality");
                if matches!(codec, VCodec::Jpeg | VCodec::Webp) {
                    ui.horizontal(|ui| {
                        let mut v = self.quality / 100.0;
                        if theme::slider(ui, &mut v, 200.0, 8.0, None).changed() {
                            self.quality = (v * 100.0).round().max(1.0);
                        }
                        ui.label(RichText::new(format!("{:.0}%  {}", self.quality, formats::quality_hint(Some(codec), self.quality))).color(p.muted));
                    });
                } else {
                    ui.label(RichText::new("Lossless").color(p.muted));
                }
                ui.end_row();

                ui.label("Resolution");
                egui::ComboBox::from_id_salt("frames-res").selected_text(self.res.label()).show_ui(ui, |ui| {
                    for r in Res::ALL {
                        ui.selectable_value(&mut self.res, r, r.label());
                    }
                });
                ui.end_row();

                ui.label("Range");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.start).hint_text("start 0:00").desired_width(90.0));
                    ui.label("→");
                    ui.add(egui::TextEdit::singleline(&mut self.end).hint_text("end").desired_width(90.0));
                });
                ui.end_row();

                ui.label("File names");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.prefix).desired_width(110.0));
                    let ext = IMAGE_SEQ_FORMATS[self.img_format].1;
                    ui.label(RichText::new(format!("000001.{ext}")).color(p.muted));
                });
                ui.end_row();

                ui.label("Save to");
                ui.horizontal(|ui| {
                    let label = self
                        .out_dir
                        .as_ref()
                        .map(|d| d.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "New folder next to the video".into());
                    ui.label(RichText::new(label).color(p.muted));
                    if theme::icon_button(ui, Icon::Folder, 30.0, false, "Choose folder").clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            self.out_dir = Some(d);
                        }
                    }
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            theme::toggle(ui, &mut self.reverse_out, "Backwards — number frames in reverse (last frame = 000001)");
            ui.add_space(8.0);

            if let Some(info) = &info {
                let (start, end) = self.range();
                let dur = end.unwrap_or(info.duration.unwrap_or(0.0)).min(info.duration.unwrap_or(f64::MAX)) - start;
                let fps = if self.original_fps { src_fps.unwrap_or(self.fps) } else { self.fps };
                let count = (dur.max(0.0) * fps).ceil() as u64;
                theme::chip(ui, &format!("≈ {count} images  ·  {} of video", util::fmt_time(dur.max(0.0))), p.accent);
                if count > 20_000 {
                    theme::hint(ui, "That is a lot of files — consider a lower fps or a shorter range.");
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::button_ex(ui, Some(Icon::Frames), "Extract frames", true, info.is_some()).clicked() {
                    match self.extract_job(info.as_ref().unwrap()) {
                        Ok(job) => {
                            self.message = Some((format!("Extracting to {}", job.output.display()), true));
                            queue.push(job);
                        }
                        Err(e) => self.message = Some((e, false)),
                    }
                }
                if let Some((m, ok)) = &self.message {
                    ui.label(RichText::new(m).color(if *ok { p.ok } else { p.danger }));
                }
            });
        });
    }

    fn extract_job(&self, info: &MediaInfo) -> Result<Job, String> {
        let source = self.video.clone().ok_or("No video selected")?;
        let (name, ext, codec) = IMAGE_SEQ_FORMATS[self.img_format];
        let stem = Path::new(&util::display_name(&source))
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "video".into());
        let parent = match &self.out_dir {
            Some(d) => d.clone(),
            None if util::is_url(&source) => util::default_output_dir("Pictures"),
            None => Path::new(&source).parent().map(PathBuf::from).unwrap_or_else(|| util::default_output_dir("Pictures")),
        };
        let dir = util::unique_dir(&parent, &format!("{stem}_frames"));
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;

        let (start, end) = self.range();
        let mut args: Vec<String> = Vec::new();
        if start > 0.0 {
            args.extend(["-ss".into(), format!("{start:.3}")]);
        }
        args.extend(["-i".into(), source.clone()]);
        if let Some(e) = end {
            args.extend(["-t".into(), format!("{:.3}", e - start)]);
        }
        args.extend(["-map".into(), "0:V:0".into(), "-an".into(), "-sn".into()]);
        let mut vf = Vec::new();
        if !self.original_fps {
            vf.push(format!("fps={:.6}", self.fps));
        }
        vf.extend(self.res.filter(info.video.as_ref().map(|v| v.height)));
        if !vf.is_empty() {
            args.extend(["-vf".into(), vf.join(",")]);
        }
        if self.original_fps {
            args.extend(["-fps_mode".into(), "passthrough".into()]);
        }
        args.extend(formats::video_args(codec, self.quality));
        if codec == VCodec::Jpeg {
            args.extend(["-pix_fmt".into(), "yuvj420p".into()]);
        }
        let prefix: String = self.prefix.chars().filter(|c| !"\\/:*?\"<>|%".contains(*c)).collect();
        args.extend(["-start_number".into(), "1".into()]);
        let pattern = dir.join(format!("{prefix}%06d.{ext}"));
        let mut os_args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
        os_args.push(pattern.into_os_string());

        let total = info.duration.map(|d| end.unwrap_or(d).min(d) - start).filter(|d| *d > 0.0);
        let fps_label = if self.original_fps { "all frames".to_string() } else { format!("{} fps", self.fps) };
        let mut job = Job::new(format!("{}  →  {name} sequence @ {fps_label}", util::display_name(&source)), dir.clone(), os_args, total);
        if self.reverse_out {
            job.post = Post::ReverseNumbering { dir, prefix, ext: ext.into() };
        }
        Ok(job)
    }

    // ----- Images → video -------------------------------------------------

    fn images_source_card(&mut self, ui: &mut Ui, w: f32) {
        let p = pal();
        theme::card(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.horizontal(|ui| {
                theme::section_title(ui, "Image sequence");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::button_icon(ui, Icon::Image, "Pick images", false).clicked() {
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Images", util::IMAGE_EXTS)
                            .pick_files()
                        {
                            self.set_images(files);
                        }
                    }
                    if theme::button_icon(ui, Icon::Folder, "Pick folder", false).clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            self.set_images(list_images(&d));
                        }
                    }
                });
            });
            if self.images.is_empty() {
                let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
                if theme::drop_zone(
                    ui,
                    vec2(ui.available_width(), 240.0),
                    "Drop a folder or many images",
                    "PNG · JPG · WebP · TIFF · BMP · EXR … sorted naturally",
                    hovering,
                )
                .clicked()
                {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.set_images(list_images(&d));
                    }
                }
            } else {
                theme::chip(ui, &format!("{} images", self.images.len()), p.accent);
                theme::well(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("seq-list").max_height(220.0).show(ui, |ui| {
                        let n = self.images.len();
                        for (i, img) in self.images.iter().enumerate() {
                            if n > 14 && i == 6 {
                                theme::hint(ui, &format!("… {} more …", n - 12));
                            }
                            if n > 14 && (6..n - 6).contains(&i) {
                                continue;
                            }
                            ui.label(
                                RichText::new(format!("{:>5}  {}", i + 1, img.file_name().unwrap_or_default().to_string_lossy()))
                                    .monospace()
                                    .color(p.text),
                            );
                        }
                    });
                });
            }
        });
    }

    fn assemble_settings(&mut self, ui: &mut Ui, queue: &mut Queue, w: f32) {
        let p = pal();
        theme::card(ui, |ui| {
            ui.set_width(w - 32.0);
            theme::section_title(ui, "Video settings");
            ui.add_space(4.0);
            egui::Grid::new("assemble-grid").num_columns(2).spacing(vec2(16.0, 12.0)).show(ui, |ui| {
                ui.label("Frames / second");
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::DragValue::new(&mut self.seq_fps).range(0.1..=240.0).speed(0.1).suffix(" fps").max_decimals(3));
                    for v in [1.0, 12.0, 24.0, 25.0, 30.0, 60.0] {
                        if pill(ui, &format!("{v}"), (self.seq_fps - v).abs() < 1e-6).clicked() {
                            self.seq_fps = v;
                        }
                    }
                });
                ui.end_row();
                ui.label("Format");
                ui.horizontal_wrapped(|ui| {
                    for (i, (name, _, _)) in SEQ_VIDEO_FORMATS.iter().enumerate() {
                        if pill(ui, name, self.seq_format == i).clicked() {
                            self.seq_format = i;
                            self.seq_output = None;
                        }
                    }
                });
                ui.end_row();
                let codec = SEQ_VIDEO_FORMATS[self.seq_format].2;
                ui.label("Quality");
                let hint = formats::quality_hint(Some(codec), self.seq_quality);
                if hint == "lossless" {
                    ui.label(RichText::new("Lossless").color(p.muted));
                } else {
                    ui.horizontal(|ui| {
                        let mut v = self.seq_quality / 100.0;
                        if theme::slider(ui, &mut v, 200.0, 8.0, None).changed() {
                            self.seq_quality = (v * 100.0).round();
                        }
                        ui.label(RichText::new(format!("{:.0}%  {hint}", self.seq_quality)).color(p.muted));
                    });
                }
                ui.end_row();
                ui.label("Save as");
                ui.horizontal(|ui| {
                    let label = self
                        .seq_output
                        .as_ref()
                        .map(|d| d.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Next to the images".into());
                    ui.label(RichText::new(label).color(p.muted));
                    if theme::icon_button(ui, Icon::Folder, 30.0, false, "Choose file").clicked() {
                        let ext = SEQ_VIDEO_FORMATS[self.seq_format].1;
                        if let Some(f) = rfd::FileDialog::new().add_filter(ext, &[ext]).set_file_name(format!("sequence.{ext}")).save_file() {
                            self.seq_output = Some(f);
                        }
                    }
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            theme::toggle(ui, &mut self.seq_reverse, "Backwards — play the sequence in reverse");
            theme::toggle(ui, &mut self.seq_boomerang, "Boomerang — forward then backward");
            ui.add_space(8.0);
            if !self.images.is_empty() {
                let frames = self.images.len() * if self.seq_boomerang { 2 } else { 1 };
                theme::chip(ui, &format!("{frames} frames  ·  {} of video", util::fmt_time(frames as f64 / self.seq_fps)), p.accent);
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                if theme::button_ex(ui, Some(Icon::Screen), "Create video", true, !self.images.is_empty()).clicked() {
                    match self.assemble_job() {
                        Ok(job) => {
                            self.message = Some((format!("Creating {}", job.output.display()), true));
                            queue.push(job);
                        }
                        Err(e) => self.message = Some((e, false)),
                    }
                }
                if let Some((m, ok)) = &self.message {
                    ui.label(RichText::new(m).color(if *ok { p.ok } else { p.danger }));
                }
            });
        });
    }

    fn assemble_job(&self) -> Result<Job, String> {
        let (name, ext, codec) = SEQ_VIDEO_FORMATS[self.seq_format];
        let mut frames: Vec<PathBuf> = self.images.clone();
        if self.seq_reverse {
            frames.reverse();
        }
        if self.seq_boomerang {
            let back: Vec<PathBuf> = frames.iter().rev().skip(1).cloned().collect();
            frames.extend(back);
        }
        let first = frames.first().ok_or("No images selected")?;
        let info = ffmpeg::probe(&first.to_string_lossy())?;
        let v = info.video.ok_or("The first file is not an image")?;
        let (w, h) = (v.width / 2 * 2, v.height / 2 * 2);

        // ffconcat list: works with any file names, orders and mixed formats.
        let list = std::env::temp_dir().join(format!("vmpc_seq_{}_{}.ffconcat", std::process::id(), rand_suffix()));
        let mut file = std::fs::File::create(&list).map_err(|e| e.to_string())?;
        let dur = 1.0 / self.seq_fps;
        let mut text = String::from("ffconcat version 1.0\n");
        for f in &frames {
            let path = f.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
            text.push_str(&format!("file '{path}'\nduration {dur:.6}\n"));
        }
        if let Some(last) = frames.last() {
            let path = last.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
            text.push_str(&format!("file '{path}'\n"));
        }
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;

        let output = match &self.seq_output {
            Some(o) => o.with_extension(ext),
            None => {
                let dir = first.parent().map(PathBuf::from).unwrap_or_else(|| util::default_output_dir("Videos"));
                let stem = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "sequence".into());
                let stem = if self.seq_reverse { format!("{stem}_reversed") } else { stem };
                let parent = dir.parent().map(PathBuf::from).unwrap_or(dir.clone());
                util::unique_path(&parent, &stem, ext)
            }
        };
        let mut vf = vec![format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"
        )];
        if codec == VCodec::Gif {
            vf.push(formats::GIF_PALETTE.to_string());
        }
        let mut args: Vec<OsString> = ["-f", "concat", "-safe", "0", "-i"].iter().map(OsString::from).collect();
        args.push(list.clone().into_os_string());
        for a in ["-vf".to_string(), vf.join(","), "-fps_mode".into(), "cfr".into(), "-r".into(), format!("{:.6}", self.seq_fps)] {
            args.push(a.into());
        }
        for a in formats::video_args(codec, self.seq_quality) {
            args.push(a.into());
        }
        if ext == "mp4" {
            args.extend(["-movflags", "+faststart"].map(OsString::from));
        }
        args.push(output.clone().into_os_string());
        let total = Some(frames.len() as f64 / self.seq_fps);
        let mut job = Job::new(format!("{} images  →  {name} @ {} fps", frames.len(), self.seq_fps), output, args, total);
        job.temp.push(list);
        Ok(job)
    }
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

fn i_duration(info: &Result<MediaInfo, String>) -> Option<f64> {
    info.as_ref().ok().and_then(|i| i.duration)
}

fn list_images(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && util::is_image_path(p)).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| util::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{testing, Status};

    fn count_files(dir: &Path, ext: &str) -> usize {
        std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == ext)).count()
    }

    #[test]
    fn extract_then_reassemble() {
        let Some(sample) = testing::sample_video() else { return };
        let src = sample.to_string_lossy().into_owned();
        let info = ffmpeg::probe(&src).unwrap();
        let out = testing::scratch_dir("frames");

        // 3 s at 5 fps as JPEG, numbered backwards.
        let f = Frames {
            video: Some(src.clone()),
            fps: 5.0,
            img_format: 1,
            quality: 80.0,
            reverse_out: true,
            out_dir: Some(out.clone()),
            ..Default::default()
        };
        let job = testing::run(f.extract_job(&info).unwrap());
        assert_eq!(job.status, Status::Done, "{:?}", job.status);
        assert_eq!(count_files(&job.output, "jpg"), 15);
        assert!(job.output.join("frame_000001.jpg").is_file());
        assert!(job.output.join("frame_000015.jpg").is_file());

        // Every original frame as PNG at 160p... (240 source, Keep).
        let all = Frames { video: Some(src.clone()), original_fps: true, img_format: 0, out_dir: Some(out.clone()), ..Default::default() };
        let job_all = testing::run(all.extract_job(&info).unwrap());
        assert_eq!(job_all.status, Status::Done);
        assert_eq!(count_files(&job_all.output, "png"), 75);

        // Back to video: 75 frames at 25 fps = 3 s, also reversed and as boomerang GIF.
        let images = list_images(&job_all.output);
        for (format, reverse, boomerang, expect) in [(0, false, false, 3.0), (0, true, false, 3.0), (4, false, true, 5.96), (2, false, false, 3.0)] {
            let a = Frames {
                images: images.clone(),
                seq_fps: 25.0,
                seq_format: format,
                seq_reverse: reverse,
                seq_boomerang: boomerang,
                seq_output: Some(out.join(format!("seq_{format}_{reverse}_{boomerang}"))),
                ..Default::default()
            };
            let job = testing::run(a.assemble_job().unwrap());
            assert_eq!(job.status, Status::Done, "{format}: {:?}", job.status);
            let probed = ffmpeg::probe(&job.output.to_string_lossy()).unwrap();
            let d = probed.duration.unwrap_or(0.0);
            assert!((d - expect).abs() < 0.2, "format {format}: duration {d}, expected {expect}");
        }
        let _ = std::fs::remove_dir_all(&out);
    }
}
