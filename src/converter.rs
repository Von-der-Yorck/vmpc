//! Everything you can turn the loaded media into, as one list of targets grouped by
//! what makes sense for the detected media type, plus the options for the selected
//! target. Video → image sequence and images → video live in the same list.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use egui::{vec2, RichText, Ui};

use crate::catalog::DataFormat;
use crate::ffmpeg::{MediaInfo, MediaKind};
use crate::formats::{self, Format, Kind, Res, FORMATS, IMAGE_SEQ_FORMATS, SEQ_VIDEO_FORMATS};
use crate::icons::Icon;
use crate::jobs::{Job, Queue};
use crate::theme::{self, pal};
use crate::util;

/// One thing the media can be converted into.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// A file format of kind `kind` (index into the Video/Audio/Image/Other kinds).
    Format { kind: usize, format: usize },
    /// Any extension FFmpeg can write.
    Custom,
    /// Video → image sequence ([`IMAGE_SEQ_FORMATS`] index).
    Sequence(usize),
    /// Image(s) → video ([`SEQ_VIDEO_FORMATS`] index).
    ToVideo(usize),
    /// Subtitle file conversion, or extraction from a video ([`formats::SUB_FORMATS`] index).
    Subtitle(usize),
    /// Data file conversion (JSON / YAML / TOML / CSV / TSV).
    Data(DataFormat),
    /// Extract an archive into a folder.
    Extract,
    /// Rewrite an archive in another format ([`crate::archive::REPACK_FORMATS`] index).
    Repack(usize),
}

/// What is being converted; decides which targets make sense.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subject {
    Media { kind: MediaKind, images: usize, text_subtitles: usize },
    Subtitle,
    Data(DataFormat),
    Archive,
}

fn subtitle_targets() -> Vec<(Target, String)> {
    formats::SUB_FORMATS.iter().enumerate().map(|(i, (name, _, _))| (Target::Subtitle(i), name.to_string())).collect()
}

/// The conversion targets that fit the subject, grouped for display.
pub fn groups_for_subject(subject: Subject) -> Vec<Group> {
    let g = |title: &str, icon, targets| Group { title: title.to_string(), icon, targets };
    match subject {
        Subject::Media { kind, images, text_subtitles } => {
            let mut groups = groups_for(kind, images);
            if text_subtitles > 0 {
                let title = if text_subtitles > 1 { format!("Subtitles ({text_subtitles} tracks)") } else { "Subtitles".into() };
                groups.insert(groups.len() - 1, g(&title, Icon::Playlist, subtitle_targets()));
            }
            groups
        }
        Subject::Subtitle => vec![g("Subtitle format", Icon::Playlist, subtitle_targets())],
        Subject::Data(from) => vec![g(
            "Data format",
            Icon::Convert,
            DataFormat::ALL.iter().filter(|f| **f != from).map(|f| (Target::Data(*f), f.label().to_string())).collect(),
        )],
        Subject::Archive => vec![
            g("Unpack", Icon::Folder, vec![(Target::Extract, "Extract all files".into())]),
            g(
                "Repack as",
                Icon::Convert,
                crate::archive::REPACK_FORMATS.iter().enumerate().map(|(i, (name, _))| (Target::Repack(i), name.to_string())).collect(),
            ),
        ],
    }
}

pub fn default_for_subject(subject: Subject) -> Target {
    match subject {
        Subject::Media { kind, images, .. } => default_target(kind, images),
        Subject::Subtitle => Target::Subtitle(0),
        Subject::Data(from) => Target::Data(if from == DataFormat::Json { DataFormat::Yaml } else { DataFormat::Json }),
        Subject::Archive => Target::Extract,
    }
}

/// Converts a subtitle file, or extracts subtitle track `track` from a video.
pub fn subtitle_job(source: &str, format: usize, track: usize, shift: f64, dir: Option<&Path>) -> Job {
    let (label, ext, codec) = formats::SUB_FORMATS[format.min(formats::SUB_FORMATS.len() - 1)];
    let mut args: Vec<OsString> = Vec::new();
    if shift.abs() > 1e-3 {
        args.extend(["-itsoffset".into(), format!("{shift:.3}").into()]);
    }
    args.extend(["-i".into(), source.into(), "-map".into(), format!("0:s:{track}").into()]);
    if let Some(c) = codec {
        args.extend(["-c:s".into(), c.into()]);
    }
    let parent = dir
        .map(PathBuf::from)
        .or_else(|| (!util::is_url(source)).then(|| Path::new(source).parent().map(PathBuf::from)).flatten())
        .unwrap_or_else(|| util::default_output_dir("Videos"));
    let name = util::display_name(source);
    let stem = Path::new(&name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "subtitles".into());
    let output = util::unique_path(&parent, &stem, ext);
    args.push(output.clone().into());
    Job::new(format!("{name} → {label} subtitles"), output, args, None)
}

pub struct Group {
    pub title: String,
    pub icon: Icon,
    pub targets: Vec<(Target, String)>,
}

fn formats_of(kind: usize) -> Vec<(Target, String)> {
    let k = kind_of(kind);
    // Index first (targets refer to the position among formats of this kind), then hide
    // formats the installed FFmpeg build cannot write.
    FORMATS
        .iter()
        .filter(|f| f.kind == k)
        .enumerate()
        .filter(|(_, f)| f.available())
        .map(|(i, f)| (Target::Format { kind, format: i }, f.name.to_string()))
        .collect()
}

/// The conversion targets that fit the detected media, grouped for display.
pub fn groups_for(media: MediaKind, images: usize) -> Vec<Group> {
    let g = |title: &str, icon, targets| Group { title: title.to_string(), icon, targets };
    let other = g("Other format", Icon::Gear, vec![(Target::Custom, "Any extension…".into())]);
    match media {
        MediaKind::Video => vec![
            g("Video", Icon::Screen, formats_of(0)),
            g("Audio only", Icon::Music, formats_of(1)),
            g("Still image", Icon::Image, formats_of(2)),
            g(
                "Image sequence",
                Icon::Frames,
                IMAGE_SEQ_FORMATS.iter().enumerate().map(|(i, (name, _, _))| (Target::Sequence(i), format!("{name} frames"))).collect(),
            ),
            other,
        ],
        MediaKind::Audio => vec![g("Audio", Icon::Music, formats_of(1)), other],
        MediaKind::Image => {
            let video_title = if images > 1 { format!("Video from {images} images") } else { "Video from this image".into() };
            let image_title = if images > 1 { format!("Image format (all {images})") } else { "Image format".into() };
            vec![
                g(&image_title, Icon::Image, formats_of(2)),
                g(
                    &video_title,
                    Icon::Screen,
                    SEQ_VIDEO_FORMATS.iter().enumerate().map(|(i, (name, _, _))| (Target::ToVideo(i), name.to_string())).collect(),
                ),
                other,
            ]
        }
    }
}

/// Sensible first choice: MP4 for video, MP3 for audio, a video for an image sequence.
pub fn default_target(media: MediaKind, images: usize) -> Target {
    match media {
        MediaKind::Video => Target::Format { kind: 0, format: 0 },
        MediaKind::Audio => Target::Format { kind: 1, format: 0 },
        MediaKind::Image if images > 1 => Target::ToVideo(0),
        MediaKind::Image => Target::Format { kind: 2, format: 1 },
    }
}

pub struct Converter {
    /// Index into the Video/Audio/Image/Other kinds.
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
    batch: bool,
}

impl Default for Converter {
    fn default() -> Self {
        Self {
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
            batch: true,
        }
    }
}

fn kind_of(i: usize) -> Kind {
    [Kind::Video, Kind::Audio, Kind::Image, Kind::Custom][i]
}

/// A label above its control.
pub fn field(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(label).size(12.5).color(pal().muted));
    ui.add_space(-2.0);
    add(ui);
    ui.add_space(6.0);
}

/// Output folder picker row.
pub fn save_to(ui: &mut Ui, dir: &mut Option<PathBuf>, default_label: &str) {
    let p = pal();
    field(ui, "Save to", |ui| {
        ui.horizontal(|ui| {
            if theme::icon_button(ui, Icon::Folder, 30.0, false, "Choose a folder").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    *dir = Some(d);
                }
            }
            let label = dir.as_ref().map(|d| theme::truncate(&d.to_string_lossy(), 34)).unwrap_or_else(|| default_label.into());
            ui.label(RichText::new(label).size(12.5).color(p.muted));
            if dir.is_some() && theme::icon_button(ui, Icon::Close, 26.0, false, "Reset").clicked() {
                *dir = None;
            }
        });
    });
}

impl Converter {
    fn selected_format(&self) -> Option<Format> {
        let kind = kind_of(self.kind);
        FORMATS.iter().filter(|f| f.kind == kind).nth(self.format).copied()
    }

    /// Selects a `Target::Format` / `Target::Custom` target.
    pub fn set_target(&mut self, target: Target) {
        match target {
            Target::Format { kind, format } => {
                self.kind = kind;
                self.format = format;
            }
            Target::Custom => self.kind = 3,
            _ => {}
        }
    }

    /// Options for the selected format; `batch_count` = loaded files of the same type.
    pub fn options_ui(&mut self, ui: &mut Ui, info: &MediaInfo, batch_count: usize) {
        let p = pal();
        let media = info.kind();
        let kind = kind_of(self.kind);
        if kind == Kind::Custom {
            field(ui, "File extension", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.custom_ext).desired_width(120.0));
                theme::hint(ui, "Any format FFmpeg can write: mkv, m4v, 3gp, ogv, ac3, amr, apng…");
            });
        }
        let fmt = self.selected_format();
        let vcodec = fmt.and_then(|f| f.video);
        let hint = formats::quality_hint(vcodec, self.quality);
        let vorbis = fmt.is_some_and(|f| matches!(f.audio, Some(formats::ACodec::Vorbis)));
        if kind != Kind::Custom && (hint != "lossless" || vorbis) && !(kind == Kind::Audio && !vorbis) {
            field(ui, "Quality", |ui| {
                ui.horizontal(|ui| {
                    let mut v = self.quality / 100.0;
                    if theme::slider(ui, &mut v, (ui.available_width() - 110.0).max(120.0), 8.0, None).changed() {
                        self.quality = (v * 100.0).round();
                    }
                    ui.label(RichText::new(format!("{:.0}%  {hint}", self.quality)).size(12.5).color(p.muted));
                });
            });
        }
        if matches!(kind, Kind::Video | Kind::Image) {
            field(ui, "Resolution", |ui| {
                egui::ComboBox::from_id_salt("conv-res").selected_text(self.res.label()).show_ui(ui, |ui| {
                    for r in Res::ALL {
                        ui.selectable_value(&mut self.res, r, r.label());
                    }
                });
            });
        }
        let bitrate = fmt.and_then(|f| f.audio).is_some_and(formats::uses_bitrate);
        if bitrate && !(kind == Kind::Video && self.no_audio) && info.audio.is_some() {
            field(ui, "Audio bitrate", |ui| {
                egui::ComboBox::from_id_salt("conv-kbps").selected_text(format!("{} kb/s", self.kbps)).show_ui(ui, |ui| {
                    for k in [64, 96, 128, 160, 192, 256, 320] {
                        ui.selectable_value(&mut self.kbps, k, format!("{k} kb/s"));
                    }
                });
            });
        }
        if media != MediaKind::Image {
            field(ui, if kind == Kind::Image { "Frame at" } else { "Trim" }, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.start).hint_text("start 0:00").desired_width(90.0));
                    if kind != Kind::Image {
                        ui.label("→");
                        ui.add(egui::TextEdit::singleline(&mut self.end).hint_text("end").desired_width(90.0));
                    }
                });
            });
            if kind != Kind::Image {
                theme::toggle(ui, &mut self.reverse, "Reverse (play backwards)");
                if self.reverse {
                    theme::hint(ui, "Buffers the whole clip in memory; trim long videos first.");
                }
            }
            if kind == Kind::Video && info.audio.is_some() {
                theme::toggle(ui, &mut self.no_audio, "Remove audio");
            }
        }
        save_to(ui, &mut self.out_dir, "Same folder as the source");
        if batch_count > 1 {
            let noun = match media {
                MediaKind::Video => "videos",
                MediaKind::Audio => "audio files",
                MediaKind::Image => "images",
            };
            theme::toggle(ui, &mut self.batch, &format!("All {batch_count} loaded {noun}"));
        }
    }

    /// Whether the start button can be used.
    pub fn ready(&self) -> bool {
        kind_of(self.kind) != Kind::Custom || !self.custom_ext.trim().is_empty()
    }

    pub fn action_label(&self, batch_count: usize) -> String {
        if self.batch && batch_count > 1 {
            format!("Convert {batch_count} files")
        } else {
            "Convert".into()
        }
    }

    /// Queues the conversion; returns a status line.
    pub fn start(&mut self, queue: &mut Queue, current: (&str, &MediaInfo), same_kind: &[(String, MediaInfo)]) -> String {
        let fmt = self.selected_format();
        let jobs: Vec<Job> = if self.batch && same_kind.len() > 1 {
            same_kind.iter().map(|(s, i)| self.build_job(s, i, fmt)).collect()
        } else {
            vec![self.build_job(current.0, current.1, fmt)]
        };
        let n = jobs.len();
        for job in jobs {
            queue.push(job);
        }
        format!("Added {n} conversion{} to the queue", if n == 1 { "" } else { "s" })
    }

    pub fn build_job(&self, source: &str, info: &MediaInfo, fmt: Option<Format>) -> Job {
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
                        if f.ext == "mxf" {
                            // MXF only carries 48 kHz audio.
                            args.extend(["-ar".into(), "48000".into()]);
                        }
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
                if f.video == Some(formats::VCodec::Ico) {
                    // Icons are at most 256 × 256.
                    vf.push("scale=256:256:force_original_aspect_ratio=decrease".into());
                }
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
        if let Some(muxer) = fmt.filter(|_| kind != Kind::Custom).and_then(|f| f.muxer) {
            args.extend(["-f".into(), muxer.into()]);
        }
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

/// Selectable format pill: outlined; selected = orange outline, wash and text.
pub fn pill(ui: &mut Ui, text: &str, selected: bool) -> egui::Response {
    let p = pal();
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), egui::Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(20.0, 12.0), egui::Sense::click());
    let hover = theme::ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.12);
    let on = theme::ease(ui.ctx(), resp.id.with("on"), selected, 0.18);
    let r = rect.height() / 2.0;
    let fill = theme::lerp_color(theme::lerp_color(p.surface, p.tint, hover), p.accent_soft, on);
    let border = theme::lerp_color(p.border, p.accent, on);
    ui.painter().add(theme::surface_shape(rect, r, fill, border));
    let color = theme::lerp_color(theme::lerp_color(p.text, p.strong, hover), p.accent, on);
    ui.painter().galley(rect.min + vec2(10.0, 6.0), galley, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Compact conversion queue (inspector and menu drawer).
pub fn jobs_list(ui: &mut Ui, queue: &mut Queue) {
    use crate::jobs::Status::*;
    let p = pal();
    let jobs = queue.snapshot();
    if jobs.is_empty() {
        theme::hint(ui, "No conversions yet. Jobs run one after another so your computer stays responsive.");
        return;
    }
    for job in jobs.iter().rev() {
        theme::well(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 40.0);
                    ui.add(egui::Label::new(RichText::new(&job.title).size(13.0).color(p.strong)).truncate());
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
                            theme::hint(ui, &format!("{:.0}% · {}{eta}", job.progress * 100.0, job.speed));
                        }
                        Done => {
                            theme::progress(ui, 1.0, ui.available_width(), true);
                            theme::hint(ui, &format!("Done in {}", util::fmt_time(job.elapsed)));
                        }
                        Failed(e) => {
                            ui.add(egui::Label::new(RichText::new(format!("Failed: {e}")).size(12.0).color(p.danger)).truncate());
                        }
                        Cancelled => theme::hint(ui, "Cancelled"),
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match job.status {
                    Queued | Running => {
                        if theme::icon_button(ui, Icon::Close, 28.0, false, "Cancel").clicked() {
                            queue.cancel(job.id);
                        }
                    }
                    Done => {
                        if theme::icon_button(ui, Icon::Folder, 28.0, false, "Show in folder").clicked() {
                            util::reveal(&job.output);
                        }
                    }
                    _ => {}
                });
            });
        });
        ui.add_space(4.0);
    }
    if jobs.iter().any(|j| !matches!(j.status, Queued | Running)) && theme::button(ui, "Clear finished", false).clicked() {
        queue.clear_finished();
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg;
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
        let (mut failures, mut skipped, mut ok) = (Vec::new(), Vec::new(), 0);
        for kind in 0..3 {
            let count = FORMATS.iter().filter(|f| f.kind == kind_of(kind)).count();
            for format in 0..count {
                let c = converter_for(kind, format, &out);
                let fmt = c.selected_format().unwrap();
                if !fmt.available() {
                    skipped.push(fmt.name);
                    continue;
                }
                let job = testing::run(c.build_job(&src, &info, Some(fmt)));
                let size = std::fs::metadata(&job.output).map(|m| m.len()).unwrap_or(0);
                // The result must be a real file FFmpeg can read back.
                let readable = ffmpeg::probe(&job.output.to_string_lossy()).is_ok();
                if job.status != Status::Done || size == 0 || !readable {
                    failures.push(format!("{}: {:?} ({size} bytes, readable: {readable})", fmt.name, job.status));
                } else {
                    ok += 1;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&out);
        eprintln!("{ok} presets converted; skipped (encoder not in this FFmpeg build): {skipped:?}");
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

    #[test]
    fn media_type_is_detected_and_only_matching_targets_are_offered() {
        let Some(sample) = testing::sample_video() else { return };
        let out = testing::scratch_dir("detect");
        let png = out.join("still.png");
        let mp3 = out.join("sound.mp3");
        for (args, target) in [(vec!["-frames:v", "1"], &png), (vec!["-vn"], &mp3)] {
            let ok = ffmpeg::ffmpeg_cmd().unwrap().args(["-y", "-v", "error", "-i"]).arg(&sample).args(args).arg(target).status().unwrap();
            assert!(ok.success());
        }
        let kind = |p: &Path| ffmpeg::probe(&p.to_string_lossy()).unwrap().kind();
        assert_eq!(kind(&sample), MediaKind::Video);
        assert_eq!(kind(&png), MediaKind::Image);
        assert_eq!(kind(&mp3), MediaKind::Audio);
        // Only fitting targets: audio never gets video/image/sequence targets; images get
        // image formats and "images → video" (even a single image); video gets sequences.
        let targets = |m, n| groups_for(m, n).into_iter().flat_map(|g| g.targets).map(|(t, _)| t).collect::<Vec<_>>();
        let audio = targets(MediaKind::Audio, 0);
        assert!(audio.iter().all(|t| matches!(t, Target::Format { kind: 1, .. } | Target::Custom)));
        let one_image = targets(MediaKind::Image, 1);
        assert!(one_image.iter().any(|t| matches!(t, Target::ToVideo(_))));
        assert!(one_image.iter().all(|t| matches!(t, Target::Format { kind: 2, .. } | Target::ToVideo(_) | Target::Custom)));
        let video = targets(MediaKind::Video, 0);
        assert!(video.iter().any(|t| matches!(t, Target::Sequence(_))));
        assert!(!video.iter().any(|t| matches!(t, Target::ToVideo(_))));
        assert_eq!(default_target(MediaKind::Image, 5), Target::ToVideo(0));
        assert_eq!(default_target(MediaKind::Audio, 0), Target::Format { kind: 1, format: 0 });
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn subtitles_convert_and_extract() {
        let Some(sample) = testing::sample_video() else { return };
        let out = testing::scratch_dir("subs");
        let srt = out.join("talk.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,500\nHello <i>there</i>\n\n2\n00:00:02,600 --> 00:00:03,000\nBye\n").unwrap();
        for (i, (name, ext, _)) in formats::SUB_FORMATS.iter().enumerate() {
            let job = testing::run(subtitle_job(&srt.to_string_lossy(), i, 0, 1.5, Some(&out)));
            assert_eq!(job.status, Status::Done, "{name}: {:?}", job.status);
            let text = std::fs::read_to_string(&job.output).unwrap();
            assert!(text.contains("Hello") || *ext == "ttml" && text.contains("Hello"), "{name} lost the text:\n{text}");
        }
        // Timing shift: +1.5 s moves 00:00:01,000 to 00:00:02,500.
        let shifted = std::fs::read_to_string(out.join("talk (2).srt")).unwrap();
        assert!(shifted.contains("00:00:02,500 --> 00:00:04,000"), "{shifted}");
        // Mux the subtitles into a video, then extract them again.
        let mkv = out.join("movie.mkv");
        let ok = ffmpeg::ffmpeg_cmd().unwrap().args(["-y", "-v", "error", "-i"]).arg(&sample).arg("-i").arg(&srt)
            .args(["-map", "0", "-map", "1", "-c", "copy", "-c:s", "srt"]).arg(&mkv).status().unwrap();
        assert!(ok.success());
        let info = ffmpeg::probe(&mkv.to_string_lossy()).unwrap();
        assert_eq!(info.subtitles, vec!["subrip".to_string()]);
        let groups = groups_for_subject(Subject::Media { kind: info.kind(), images: 0, text_subtitles: 1 });
        assert!(groups.iter().any(|g| g.targets.iter().any(|(t, _)| matches!(t, Target::Subtitle(_)))));
        let job = testing::run(subtitle_job(&mkv.to_string_lossy(), 1, 0, 0.0, Some(&out)));
        assert_eq!(job.status, Status::Done, "{:?}", job.status);
        assert!(std::fs::read_to_string(&job.output).unwrap().starts_with("WEBVTT"));
        let _ = std::fs::remove_dir_all(&out);
    }
}
