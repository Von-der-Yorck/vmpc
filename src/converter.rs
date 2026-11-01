//! Conversion panel in the inspector. It only offers targets that make sense for the
//! detected media type and converts the current file (or every file of that type).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use egui::{vec2, RichText, Ui};

use crate::ffmpeg::{MediaInfo, MediaKind};
use crate::formats::{self, Format, Kind, Res, FORMATS};
use crate::icons::Icon;
use crate::jobs::{Job, Queue};
use crate::theme::{self, pal};
use crate::util;

pub struct Converter {
    /// Index into [`ALL_KINDS`].
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
    message: Option<String>,
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
            batch: false,
            message: None,
        }
    }
}

const ALL_KINDS: [(Icon, &str); 4] = [(Icon::Screen, "Video"), (Icon::Music, "Audio"), (Icon::Image, "Image"), (Icon::Gear, "Other")];

fn kind_of(i: usize) -> Kind {
    [Kind::Video, Kind::Audio, Kind::Image, Kind::Custom][i]
}

/// Output kinds that make sense for a source of the given type (indices into [`ALL_KINDS`]).
pub fn allowed_kinds(media: MediaKind) -> &'static [usize] {
    match media {
        // Video → video, extract the soundtrack, grab a frame, or anything else.
        MediaKind::Video => &[0, 1, 2, 3],
        MediaKind::Audio => &[1, 3],
        MediaKind::Image => &[2, 3],
    }
}

/// A label above its control: reads well in the narrow inspector.
pub fn field(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(label).size(12.0).color(pal().muted));
    ui.add_space(-4.0);
    add(ui);
    ui.add_space(4.0);
}

impl Converter {
    fn selected_format(&self) -> Option<Format> {
        let kind = kind_of(self.kind);
        FORMATS.iter().filter(|f| f.kind == kind).nth(self.format).copied()
    }

    /// `current` is the file in the preview; `same_kind` are all loaded files of its type.
    pub fn panel(&mut self, ui: &mut Ui, queue: &mut Queue, current: (&str, &MediaInfo), same_kind: &[(String, MediaInfo)]) {
        let p = pal();
        let media = current.1.kind();
        let allowed = allowed_kinds(media);
        if !allowed.contains(&self.kind) {
            self.kind = allowed[0];
            self.format = 0;
        }

        field(ui, "Convert to", |ui| {
            let items: Vec<(Icon, &str)> = allowed.iter().map(|&k| ALL_KINDS[k]).collect();
            let mut sel = allowed.iter().position(|&k| k == self.kind).unwrap_or(0);
            if theme::segmented(ui, &mut sel, &items) {
                self.kind = allowed[sel];
                self.format = 0;
            }
        });
        let kind = kind_of(self.kind);
        if kind == Kind::Custom {
            field(ui, "File extension", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.custom_ext).desired_width(120.0));
                theme::hint(ui, "Any format FFmpeg can write: mkv, m4v, 3gp, ogv, ac3, amr, apng…");
            });
        } else {
            field(ui, "Format", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 10.0);
                    for (i, fmt) in FORMATS.iter().filter(|f| f.kind == kind).enumerate() {
                        if pill(ui, fmt.name, self.format == i).clicked() {
                            self.format = i;
                        }
                    }
                });
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
        let bitrate = fmt.and_then(|f| f.audio).is_some_and(|a| {
            use formats::ACodec::*;
            matches!(a, Mp3 | Aac | Opus | Wma | Mp2)
        });
        if bitrate && !(kind == Kind::Video && self.no_audio) && current.1.audio.is_some() {
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
            if kind == Kind::Video && current.1.audio.is_some() {
                theme::toggle(ui, &mut self.no_audio, "Remove audio");
            }
        }
        field(ui, "Save to", |ui| {
            ui.horizontal(|ui| {
                if theme::icon_button(ui, Icon::Folder, 30.0, false, "Choose output folder").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.out_dir = Some(d);
                    }
                }
                let label = self
                    .out_dir
                    .as_ref()
                    .map(|d| theme::truncate(&d.to_string_lossy(), 34))
                    .unwrap_or_else(|| "Same folder as the source".into());
                ui.label(RichText::new(label).size(12.5).color(p.muted));
                if self.out_dir.is_some() && theme::icon_button(ui, Icon::Close, 26.0, false, "Reset").clicked() {
                    self.out_dir = None;
                }
            });
        });
        if same_kind.len() > 1 {
            let noun = match media {
                MediaKind::Video => "videos",
                MediaKind::Audio => "audio files",
                MediaKind::Image => "images",
            };
            theme::toggle(ui, &mut self.batch, &format!("Convert all {} loaded {noun}", same_kind.len()));
        }
        ui.add_space(8.0);
        let enabled = kind != Kind::Custom || !self.custom_ext.trim().is_empty();
        let count = if self.batch && same_kind.len() > 1 { same_kind.len() } else { 1 };
        let label = if count > 1 { format!("Convert {count} files") } else { "Convert".to_string() };
        if theme::button_ex(ui, Some(Icon::Convert), &label, true, enabled).clicked() {
            let jobs: Vec<Job> = if count > 1 {
                same_kind.iter().map(|(s, i)| self.build_job(s, i, fmt)).collect()
            } else {
                vec![self.build_job(current.0, current.1, fmt)]
            };
            let n = jobs.len();
            for job in jobs {
                queue.push(job);
            }
            self.message = Some(format!("Added {n} job{} to the queue", if n == 1 { "" } else { "s" }));
        }
        if let Some(m) = &self.message {
            ui.label(RichText::new(m).size(12.5).color(p.ok));
        }
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
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), egui::Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(22.0, 14.0), egui::Sense::click());
    let hover = theme::ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.15);
    let on = theme::ease(ui.ctx(), resp.id.with("on"), selected, 0.2);
    let r = rect.height() / 2.0;
    if on > 0.5 {
        theme::paint_inset(ui.painter(), rect, r, 3.0);
    } else {
        ui.painter().extend(theme::raised_shapes(rect, r, 3.5 - 1.5 * hover, p.bg));
    }
    let color = theme::lerp_color(theme::lerp_color(p.text, p.strong, hover), p.accent, on);
    ui.painter().galley(rect.min + vec2(11.0, 7.0), galley, color);
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
        // Audio never offers video or image targets; images never offer video or audio.
        assert_eq!(allowed_kinds(MediaKind::Audio), &[1, 3]);
        assert_eq!(allowed_kinds(MediaKind::Image), &[2, 3]);
        assert_eq!(allowed_kinds(MediaKind::Video), &[0, 1, 2, 3]);
        let _ = std::fs::remove_dir_all(&out);
    }
}
