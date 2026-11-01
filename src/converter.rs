//! "Convert" tab: batch conversion of any media file (or URL) to another format.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use egui::{vec2, RichText, Ui};

use crate::ffmpeg::{self, MediaInfo};
use crate::formats::{self, Format, Kind, Res, FORMATS};
use crate::icons::Icon;
use crate::jobs::{Job, Queue};
use crate::theme::{self, pal};
use crate::util;

pub struct Input {
    pub source: String,
    pub info: Option<Result<MediaInfo, String>>,
}

pub struct Converter {
    pub inputs: Vec<Input>,
    kind: usize,
    format: usize,
    custom_ext: String,
    quality: f32,
    res: Res,
    kbps: u32,
    start: String,
    end: String,
    reverse: bool,
    no_audio: bool,
    out_dir: Option<PathBuf>,
    url: String,
    probe_tx: Sender<(String, Result<MediaInfo, String>)>,
    probe_rx: Receiver<(String, Result<MediaInfo, String>)>,
    message: Option<String>,
}

impl Default for Converter {
    fn default() -> Self {
        let (probe_tx, probe_rx) = mpsc::channel();
        Self {
            inputs: Vec::new(),
            kind: 0,
            format: 0,
            custom_ext: "mkv".into(),
            quality: 70.0,
            res: Res::Keep,
            kbps: 192,
            start: String::new(),
            end: String::new(),
            reverse: false,
            no_audio: false,
            out_dir: None,
            url: String::new(),
            probe_tx,
            probe_rx,
            message: None,
        }
    }
}

const KINDS: [(Icon, &str); 4] = [(Icon::Screen, "Video"), (Icon::Music, "Audio"), (Icon::Image, "Image"), (Icon::Gear, "Custom")];

fn kind_of(i: usize) -> Kind {
    [Kind::Video, Kind::Audio, Kind::Image, Kind::Custom][i]
}

impl Converter {
    pub fn add(&mut self, source: String, ctx: &egui::Context) {
        if self.inputs.iter().any(|i| i.source == source) {
            return;
        }
        let tx = self.probe_tx.clone();
        let src = source.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((src.clone(), ffmpeg::probe(&src)));
            ctx.request_repaint();
        });
        self.inputs.push(Input { source, info: None });
    }

    fn selected_format(&self) -> Option<Format> {
        let kind = kind_of(self.kind);
        FORMATS.iter().filter(|f| f.kind == kind).nth(self.format).copied()
    }

    pub fn ui(&mut self, ui: &mut Ui, queue: &mut Queue) {
        while let Ok((src, res)) = self.probe_rx.try_recv() {
            if let Some(i) = self.inputs.iter_mut().find(|i| i.source == src) {
                i.info = Some(res);
            }
        }
        let wide = ui.available_width() > 900.0;
        let col_w = if wide { (ui.available_width() - 30.0) / 2.0 } else { ui.available_width() - 8.0 };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            if wide {
                ui.horizontal_top(|ui| {
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        self.inputs_card(ui, col_w);
                        ui.add_space(14.0);
                        jobs_card(ui, queue, col_w);
                    });
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        self.settings_card(ui, queue, col_w);
                    });
                });
            } else {
                ui.vertical(|ui| {
                    self.inputs_card(ui, col_w);
                    ui.add_space(14.0);
                    self.settings_card(ui, queue, col_w);
                    ui.add_space(14.0);
                    jobs_card(ui, queue, col_w);
                });
            }
            ui.add_space(20.0);
        });
    }

    fn inputs_card(&mut self, ui: &mut Ui, w: f32) {
        let p = pal();
        theme::card(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.horizontal(|ui| {
                theme::section_title(ui, "Source files");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.inputs.is_empty() && theme::button_icon(ui, Icon::Trash, "Clear", false).clicked() {
                        self.inputs.clear();
                    }
                    if theme::button_icon(ui, Icon::Plus, "Add files", false).clicked() {
                        if let Some(files) = rfd::FileDialog::new().set_title("Add files to convert").pick_files() {
                            for f in files {
                                self.add(f.to_string_lossy().into_owned(), ui.ctx());
                            }
                        }
                    }
                });
            });
            ui.add_space(4.0);
            if self.inputs.is_empty() {
                let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
                let zone = theme::drop_zone(
                    ui,
                    vec2(ui.available_width(), 230.0),
                    "Drop any media files here",
                    "video · audio · images — or click to browse",
                    hovering,
                );
                if zone.clicked() {
                    if let Some(files) = rfd::FileDialog::new().pick_files() {
                        for f in files {
                            self.add(f.to_string_lossy().into_owned(), ui.ctx());
                        }
                    }
                }
            } else {
                let mut remove = None;
                theme::well(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("conv-inputs").max_height(260.0).show(ui, |ui| {
                        for (idx, input) in self.inputs.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let icon = match &input.info {
                                    Some(Ok(i)) if i.video.is_some() && !i.is_still() => Icon::Screen,
                                    Some(Ok(i)) if i.is_still() => Icon::Image,
                                    Some(Ok(_)) => Icon::Music,
                                    _ => Icon::Folder,
                                };
                                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                                crate::icons::paint(ui.painter(), r, icon, p.accent);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(util::display_name(&input.source)).color(p.strong));
                                    let (text, color) = match &input.info {
                                        None => ("Analysing…".to_string(), p.muted),
                                        Some(Ok(i)) => (
                                            format!("{}  ·  {}", util::fmt_time(i.duration.unwrap_or(0.0)), i.summary()),
                                            p.muted,
                                        ),
                                        Some(Err(e)) => (e.clone(), p.danger),
                                    };
                                    ui.label(RichText::new(text).size(12.0).color(color));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if theme::icon_button(ui, Icon::Close, 28.0, false, "Remove").clicked() {
                                        remove = Some(idx);
                                    }
                                });
                            });
                            ui.add_space(4.0);
                        }
                    });
                });
                if let Some(i) = remove {
                    self.inputs.remove(i);
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                crate::icons::paint(ui.painter(), r, Icon::Globe, p.text);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("…or paste a URL (http, https, rtmp, rtsp, HLS)")
                        .desired_width(ui.available_width() - 90.0),
                );
                let go = theme::button(ui, "Add", false).clicked()
                    || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.url.trim().is_empty() {
                    let url = self.url.trim().to_string();
                    self.add(url, ui.ctx());
                    self.url.clear();
                }
            });
        });
    }

    fn settings_card(&mut self, ui: &mut Ui, queue: &mut Queue, w: f32) {
        let p = pal();
        theme::card(ui, |ui| {
            ui.set_width(w - 32.0);
            theme::section_title(ui, "Convert to");
            ui.add_space(2.0);
            if theme::segmented(ui, &mut self.kind, &KINDS) {
                self.format = 0;
            }
            ui.add_space(6.0);
            let kind = kind_of(self.kind);
            if kind == Kind::Custom {
                ui.horizontal(|ui| {
                    ui.label("File extension");
                    ui.add(egui::TextEdit::singleline(&mut self.custom_ext).desired_width(90.0));
                });
                theme::hint(
                    ui,
                    "Any container FFmpeg can write (mkv, mp4, flac, m4v, 3gp, ogv, ac3, amr, apng, …). Codecs are chosen automatically.",
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 12.0);
                    for (i, fmt) in FORMATS.iter().filter(|f| f.kind == kind).enumerate() {
                        if pill(ui, fmt.name, self.format == i).clicked() {
                            self.format = i;
                        }
                    }
                });
            }
            let fmt = self.selected_format();
            ui.add_space(10.0);

            egui::Grid::new("conv-grid").num_columns(2).spacing(vec2(16.0, 12.0)).show(ui, |ui| {
                let vcodec = fmt.and_then(|f| f.video);
                let lossless = matches!(formats::quality_hint(vcodec, 50.0).as_str(), "lossless");
                let uses_quality = kind != Kind::Custom
                    && (!lossless || fmt.is_some_and(|f| matches!(f.audio, Some(formats::ACodec::Vorbis))));
                if uses_quality {
                    ui.label("Quality");
                    ui.horizontal(|ui| {
                        let mut v = self.quality / 100.0;
                        if theme::slider(ui, &mut v, 200.0, 8.0, None).changed() {
                            self.quality = (v * 100.0).round();
                        }
                        let hint = formats::quality_hint(vcodec, self.quality);
                        ui.label(RichText::new(format!("{:.0}%  {hint}", self.quality)).color(p.muted));
                    });
                    ui.end_row();
                }
                if matches!(kind, Kind::Video | Kind::Image) {
                    ui.label("Resolution");
                    egui::ComboBox::from_id_salt("conv-res").selected_text(self.res.label()).show_ui(ui, |ui| {
                        for r in Res::ALL {
                            ui.selectable_value(&mut self.res, r, r.label());
                        }
                    });
                    ui.end_row();
                }
                let bitrate_codec = fmt.and_then(|f| f.audio).filter(|a| {
                    use formats::ACodec::*;
                    matches!(a, Mp3 | Aac | Opus | Wma | Mp2)
                });
                if bitrate_codec.is_some() && !(kind == Kind::Video && self.no_audio) {
                    ui.label("Audio bitrate");
                    egui::ComboBox::from_id_salt("conv-kbps").selected_text(format!("{} kb/s", self.kbps)).show_ui(ui, |ui| {
                        for k in [64, 96, 128, 160, 192, 256, 320] {
                            ui.selectable_value(&mut self.kbps, k, format!("{k} kb/s"));
                        }
                    });
                    ui.end_row();
                }
                ui.label(if kind == Kind::Image { "Frame at" } else { "Trim" });
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.start).hint_text("start 0:00").desired_width(90.0));
                    if kind != Kind::Image {
                        ui.label("→");
                        ui.add(egui::TextEdit::singleline(&mut self.end).hint_text("end").desired_width(90.0));
                    }
                });
                ui.end_row();
                ui.label("Save to");
                ui.horizontal(|ui| {
                    let label = self
                        .out_dir
                        .as_ref()
                        .map(|d| d.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Same folder as source".into());
                    ui.label(RichText::new(label).color(p.muted));
                    if theme::icon_button(ui, Icon::Folder, 30.0, false, "Choose output folder").clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            self.out_dir = Some(d);
                        }
                    }
                    if self.out_dir.is_some() && theme::icon_button(ui, Icon::Close, 30.0, false, "Reset").clicked() {
                        self.out_dir = None;
                    }
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            if matches!(kind, Kind::Video | Kind::Audio | Kind::Custom) {
                ui.horizontal_wrapped(|ui| {
                    theme::toggle(ui, &mut self.reverse, "Reverse (play backwards)");
                    ui.add_space(12.0);
                    if kind != Kind::Audio {
                        theme::toggle(ui, &mut self.no_audio, "Remove audio");
                    }
                });
                if self.reverse {
                    theme::hint(ui, "Reversing buffers the whole clip in RAM — trim long videos first.");
                }
            }
            ui.add_space(10.0);
            let ready = self.inputs.iter().filter(|i| matches!(i.info, Some(Ok(_)))).count();
            let label = match ready {
                0 => "Convert".to_string(),
                1 => "Convert 1 file".to_string(),
                n => format!("Convert {n} files"),
            };
            ui.horizontal(|ui| {
                let enabled = ready > 0 &&(kind != Kind::Custom || !self.custom_ext.trim().is_empty());
                if theme::button_ex(ui, Some(Icon::Convert), &label, true, enabled).clicked() {
                    let mut count = 0;
                    let jobs: Vec<Job> = self
                        .inputs
                        .iter()
                        .filter_map(|i| match &i.info {
                            Some(Ok(info)) => Some(self.build_job(&i.source, info, fmt)),
                            _ => None,
                        })
                        .collect();
                    for job in jobs {
                        queue.push(job);
                        count += 1;
                    }
                    self.message = Some(format!("Queued {count} conversion(s)"));
                }
                if let Some(m) = &self.message {
                    ui.label(RichText::new(m).color(p.ok));
                }
            });
        });
    }

    fn build_job(&self, source: &str, info: &MediaInfo, fmt: Option<Format>) -> Job {
        let kind = kind_of(self.kind);
        let ext = match fmt {
            Some(f) if kind != Kind::Custom => f.ext.to_string(),
            _ => self.custom_ext.trim().trim_start_matches('.').to_string(),
        };
        let start = util::parse_time(&self.start).unwrap_or(0.0);
        let end = util::parse_time(&self.end).filter(|e| *e > start);
        let mut args: Vec<String> = Vec::new();
        if start > 0.0 {
            args.extend(["-ss".into(), format!("{start:.3}")]);
        }
        args.extend(["-i".into(), source.into()]);
        if let Some(e) = end {
            if kind != Kind::Image {
                args.extend(["-t".into(), format!("{:.3}", e - start)]);
            }
        }
        let src_h = info.video.as_ref().map(|v| v.height);
        let mut vf: Vec<String> = Vec::new();
        let mut af: Vec<String> = Vec::new();
        if self.reverse && kind != Kind::Image {
            if info.video.is_some() && kind != Kind::Audio {
                vf.push("reverse".into());
            }
            if info.audio.is_some() && !self.no_audio {
                af.push("areverse".into());
            }
        }
        if matches!(kind, Kind::Video | Kind::Image) {
            vf.extend(self.res.filter(src_h));
        }
        let q = self.quality;
        match (kind, fmt) {
            (Kind::Video, Some(f)) => {
                args.extend(["-map".into(), "0:V:0?".into()]);
                let v = f.video.unwrap();
                if v == formats::VCodec::Gif {
                    vf.push(formats::gif_filter(q));
                }
                if !vf.is_empty() {
                    args.extend(["-vf".into(), vf.join(",")]);
                }
                args.extend(formats::video_args(v, q));
                match f.audio {
                    Some(a) if !self.no_audio && info.audio.is_some() => {
                        args.extend(["-map".into(), "0:a:0?".into()]);
                        args.extend(formats::audio_args(a, q, self.kbps));
                        if !af.is_empty() {
                            args.extend(["-af".into(), af.join(",")]);
                        }
                    }
                    _ => args.push("-an".into()),
                }
                args.push("-sn".into());
                if f.ext == "mp4" {
                    args.extend(["-movflags".into(), "+faststart".into()]);
                }
            }
            (Kind::Audio, Some(f)) => {
                args.extend(["-map".into(), "0:a:0".into(), "-vn".into(), "-sn".into()]);
                args.extend(formats::audio_args(f.audio.unwrap(), q, self.kbps));
                if !af.is_empty() {
                    args.extend(["-af".into(), af.join(",")]);
                }
            }
            (Kind::Image, Some(f)) => {
                args.extend(["-map".into(), "0:v:0".into(), "-frames:v".into(), "1".into(), "-update".into(), "1".into()]);
                if !vf.is_empty() {
                    args.extend(["-vf".into(), vf.join(",")]);
                }
                args.extend(formats::video_args(f.video.unwrap(), q));
            }
            _ => {
                if !vf.is_empty() {
                    args.extend(["-vf".into(), vf.join(",")]);
                }
                if self.no_audio {
                    args.push("-an".into());
                } else if !af.is_empty() {
                    args.extend(["-af".into(), af.join(",")]);
                }
            }
        }

        let dir = self.out_dir.clone().unwrap_or_else(|| {
            if util::is_url(source) {
                util::default_output_dir("Videos")
            } else {
                Path::new(source).parent().map(PathBuf::from).unwrap_or_else(|| util::default_output_dir("Videos"))
            }
        });
        let name = util::display_name(source);
        let stem = Path::new(&name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty() && !s.contains('/'))
            .unwrap_or_else(|| "converted".into());
        let stem = if self.reverse { format!("{stem}_reversed") } else { stem };
        let output = util::unique_path(&dir, &stem, &ext);
        let mut os_args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
        os_args.push(output.clone().into_os_string());

        let total = if kind == Kind::Image {
            None
        } else {
            info.duration.map(|d| end.unwrap_or(d).min(d) - start).filter(|d| *d > 0.0)
        };
        let label = fmt.map(|f| f.name.to_string()).unwrap_or_else(|| ext.to_uppercase());
        Job::new(format!("{name}  →  {label}"), output, os_args, total)
    }
}

/// Selectable pill (raised when off, pressed-in and accented when on).
pub fn pill(ui: &mut Ui, text: &str, selected: bool) -> egui::Response {
    let p = pal();
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(13.5), p.text);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(26.0, 16.0), egui::Sense::click());
    let hover = resp.hovered();
    if selected {
        theme::paint_inset(ui.painter(), rect, rect.height() / 2.0, 3.0);
    } else {
        ui.painter().extend(theme::raised_shapes(rect, rect.height() / 2.0, if hover { 2.5 } else { 3.5 }, p.bg));
    }
    let color = if selected { p.accent } else if hover { p.strong } else { p.text };
    ui.painter().galley(rect.min + vec2(13.0, 8.0), galley, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Shared job list (used by both Convert and Frames tabs).
pub fn jobs_card(ui: &mut Ui, queue: &mut Queue, w: f32) {
    let p = pal();
    let jobs = queue.snapshot();
    theme::card(ui, |ui| {
        ui.set_width(w - 32.0);
        ui.horizontal(|ui| {
            theme::section_title(ui, "Queue");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if jobs.iter().any(|j| !matches!(j.status, crate::jobs::Status::Queued | crate::jobs::Status::Running))
                    && theme::button(ui, "Clear finished", false).clicked()
                {
                    queue.clear_finished();
                }
            });
        });
        if jobs.is_empty() {
            theme::hint(ui, "Nothing queued yet. Jobs run one after another so your computer stays responsive.");
            return;
        }
        for job in jobs.iter().rev() {
            theme::well(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() - 90.0);
                        ui.label(RichText::new(&job.title).color(p.strong));
                        use crate::jobs::Status::*;
                        match &job.status {
                            Queued => theme::hint(ui, "Waiting…"),
                            Running => {
                                theme::progress(ui, job.progress, ui.available_width(), false);
                                let eta = if job.progress > 0.01 {
                                    let rest = job.elapsed / job.progress as f64 * (1.0 - job.progress as f64);
                                    format!(" · ~{} left", util::fmt_time(rest))
                                } else {
                                    String::new()
                                };
                                theme::hint(
                                    ui,
                                    &format!("{:.0}%  ·  {}  ·  speed {}{eta}", job.progress * 100.0, util::fmt_time(job.elapsed), job.speed),
                                );
                            }
                            Done => {
                                theme::progress(ui, 1.0, ui.available_width(), true);
                                ui.label(
                                    RichText::new(format!("Done in {} → {}", util::fmt_time(job.elapsed), job.output.display()))
                                        .size(12.0)
                                        .color(p.ok),
                                );
                            }
                            Failed(e) => {
                                ui.label(RichText::new(format!("Failed: {e}")).size(12.0).color(p.danger));
                            }
                            Cancelled => theme::hint(ui, "Cancelled"),
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match job.status {
                        crate::jobs::Status::Queued | crate::jobs::Status::Running => {
                            if theme::icon_button(ui, Icon::Close, 30.0, false, "Cancel").clicked() {
                                queue.cancel(job.id);
                            }
                        }
                        crate::jobs::Status::Done => {
                            if theme::icon_button(ui, Icon::Folder, 30.0, false, "Show in folder").clicked() {
                                util::reveal(&job.output);
                            }
                        }
                        _ => {}
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{testing, Status};

    fn converter_for(kind: usize, format: usize, out: &Path) -> Converter {
        Converter { kind, format, out_dir: Some(out.to_path_buf()), ..Default::default() }
    }

    #[test]
    fn every_preset_converts_the_sample() {
        let Some(sample) = testing::sample_video() else { return };
        let src = sample.to_string_lossy().into_owned();
        let info = ffmpeg::probe(&src).unwrap();
        let out = testing::scratch_dir("convert_all");
        let mut failures = Vec::new();
        for kind in 0..3 {
            let count = FORMATS.iter().filter(|f| f.kind == kind_of(kind)).count();
            for format in 0..count {
                let c = converter_for(kind, format, &out);
                let fmt = c.selected_format();
                let job = testing::run(c.build_job(&src, &info, fmt));
                let size = std::fs::metadata(&job.output).map(|m| m.len()).unwrap_or(0);
                if job.status != Status::Done || size == 0 {
                    failures.push(format!("{}: {:?} ({size} bytes)", fmt.unwrap().name, job.status));
                }
            }
        }
        let _ = std::fs::remove_dir_all(&out);
        assert!(failures.is_empty(), "failed presets: {failures:#?}");
    }

    #[test]
    fn trim_reverse_and_downscale() {
        let Some(sample) = testing::sample_video() else { return };
        let src = sample.to_string_lossy().into_owned();
        let info = ffmpeg::probe(&src).unwrap();
        let out = testing::scratch_dir("convert_opts");
        let mut c = converter_for(0, 0, &out);
        c.start = "0.5".into();
        c.end = "2.5".into();
        c.reverse = true;
        c.res = Res::P360;
        let job = testing::run(c.build_job(&src, &info, c.selected_format()));
        assert_eq!(job.status, Status::Done);
        assert!(job.output.file_name().unwrap().to_string_lossy().contains("_reversed"));
        let result = ffmpeg::probe(&job.output.to_string_lossy()).unwrap();
        let d = result.duration.unwrap();
        assert!((d - 2.0).abs() < 0.15, "duration {d}");
        assert_eq!(result.video.unwrap().height, 240, "never upscales");

        let mut custom = converter_for(3, 0, &out);
        custom.custom_ext = "mkv".into();
        custom.no_audio = true;
        let job = testing::run(custom.build_job(&src, &info, None));
        assert_eq!(job.status, Status::Done);
        assert!(ffmpeg::probe(&job.output.to_string_lossy()).unwrap().audio.is_none());
        let _ = std::fs::remove_dir_all(&out);
    }
}
