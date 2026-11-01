//! Image-sequence conversions: video → images (like `ffmpeg -vf fps=N out_%06d.png`)
//! and images → video (a sequence at N fps, or a still/slideshow at N seconds per
//! image), both optionally backwards. The UI shows these as targets in the same list
//! as every other format; this module provides their options and jobs.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use egui::{vec2, RichText, Ui};

use crate::converter::{field, pill, save_to};
use crate::ffmpeg::{self, MediaInfo};
use crate::formats::{self, Res, VCodec, IMAGE_SEQ_FORMATS, SEQ_VIDEO_FORMATS};
use crate::jobs::{Job, Post, Queue};
use crate::theme::{self, pal};
use crate::util;

pub struct Frames {
    // Video → images
    video: Option<String>,
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
    /// "Seconds per image" (slideshow) instead of frames per second.
    seq_still: bool,
    seq_seconds: f64,
    seq_output: Option<PathBuf>,
}

impl Default for Frames {
    fn default() -> Self {
        Self {
            video: None,
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
            seq_still: false,
            seq_seconds: 3.0,
            seq_output: None,
        }
    }
}

impl Frames {
    pub fn set_sequence_format(&mut self, i: usize) {
        self.img_format = i.min(IMAGE_SEQ_FORMATS.len() - 1);
    }

    pub fn set_video_format(&mut self, i: usize) {
        if self.seq_format != i {
            self.seq_output = None;
        }
        self.seq_format = i.min(SEQ_VIDEO_FORMATS.len() - 1);
    }

    /// Stills: one picture always, or many in "seconds per image" mode.
    fn still_mode(&self) -> bool {
        self.images.len() == 1 || self.seq_still
    }

    // ----- Video → images -------------------------------------------------

    pub fn extract_options_ui(&mut self, ui: &mut Ui, info: &MediaInfo) {
        let p = pal();
        let src_fps = info.video.as_ref().map(|v| v.fps);
        field(ui, "Frames per second", |ui| {
            theme::toggle(ui, &mut self.original_fps, "Every frame (original rate)");
            if !self.original_fps {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    ui.add(egui::DragValue::new(&mut self.fps).range(0.01..=240.0).speed(0.1).suffix(" fps").max_decimals(3));
                    for v in [1.0, 5.0, 10.0, 24.0, 30.0, 60.0] {
                        if pill(ui, &format!("{v}"), (self.fps - v).abs() < 1e-6).clicked() {
                            self.fps = v;
                        }
                    }
                });
            }
        });
        let codec = IMAGE_SEQ_FORMATS[self.img_format].2;
        if matches!(codec, VCodec::Jpeg | VCodec::Webp) {
            field(ui, "Quality", |ui| {
                ui.horizontal(|ui| {
                    let mut v = self.quality / 100.0;
                    if theme::slider(ui, &mut v, (ui.available_width() - 110.0).max(120.0), 8.0, None).changed() {
                        self.quality = (v * 100.0).round().max(1.0);
                    }
                    let hint = formats::quality_hint(Some(codec), self.quality);
                    ui.label(RichText::new(format!("{:.0}%  {hint}", self.quality)).size(12.5).color(p.muted));
                });
            });
        }
        field(ui, "Resolution", |ui| {
            egui::ComboBox::from_id_salt("frames-res").selected_text(self.res.label()).show_ui(ui, |ui| {
                for r in Res::ALL {
                    ui.selectable_value(&mut self.res, r, r.label());
                }
            });
        });
        field(ui, "Range", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.start).hint_text("start 0:00").desired_width(90.0));
                ui.label("→");
                ui.add(egui::TextEdit::singleline(&mut self.end).hint_text("end").desired_width(90.0));
            });
        });
        field(ui, "File names", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.prefix).desired_width(100.0));
                let ext = IMAGE_SEQ_FORMATS[self.img_format].1;
                ui.label(RichText::new(format!("000001.{ext}")).size(12.5).color(p.muted));
            });
        });
        theme::toggle(ui, &mut self.reverse_out, "Backwards (last frame becomes 000001)");
        ui.add_space(4.0);
        save_to(ui, &mut self.out_dir, "New folder next to the video");
        let (start, end) = self.range();
        let dur = end.unwrap_or(info.duration.unwrap_or(0.0)).min(info.duration.unwrap_or(f64::MAX)) - start;
        let fps = if self.original_fps { src_fps.unwrap_or(self.fps) } else { self.fps };
        let count = (dur.max(0.0) * fps).ceil() as u64;
        theme::chip(ui, &format!("≈ {count} images from {}", util::fmt_time(dur.max(0.0))), p.accent);
        if count > 20_000 {
            theme::hint(ui, "That is a lot of files; consider a lower fps or a shorter range.");
        }
    }

    pub fn start_extract(&mut self, queue: &mut Queue, source: &str, info: &MediaInfo) -> Result<String, String> {
        self.video = Some(source.to_string());
        let job = self.extract_job(info)?;
        let msg = format!("Extracting frames to {}", job.output.display());
        queue.push(job);
        Ok(msg)
    }

    // ----- Images → video -------------------------------------------------

    fn sync_images(&mut self, images: &[PathBuf]) {
        let mut sorted = images.to_vec();
        sorted.sort_by(|a, b| util::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
        if sorted != self.images {
            self.images = sorted;
            self.seq_output = None;
        }
    }

    pub fn assemble_options_ui(&mut self, ui: &mut Ui, images: &[PathBuf]) {
        let p = pal();
        self.sync_images(images);
        let many = self.images.len() > 1;
        if many {
            field(ui, "Timing", |ui| {
                let mut mode = usize::from(self.seq_still);
                if theme::segmented(ui, &mut mode, &[(crate::icons::Icon::Frames, "Frames per second"), (crate::icons::Icon::Image, "Seconds per image")]) {
                    self.seq_still = mode == 1;
                }
            });
        }
        if self.still_mode() {
            field(ui, if many { "Seconds per image" } else { "Video length (seconds)" }, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    ui.add(egui::DragValue::new(&mut self.seq_seconds).range(0.1..=3600.0).speed(0.1).suffix(" s").max_decimals(2));
                    for v in [1.0, 3.0, 5.0, 10.0, 30.0] {
                        if pill(ui, &format!("{v} s"), (self.seq_seconds - v).abs() < 1e-6).clicked() {
                            self.seq_seconds = v;
                        }
                    }
                });
            });
        } else {
            field(ui, "Frames per second", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    ui.add(egui::DragValue::new(&mut self.seq_fps).range(0.1..=240.0).speed(0.1).suffix(" fps").max_decimals(3));
                    for v in [1.0, 12.0, 24.0, 30.0, 60.0] {
                        if pill(ui, &format!("{v}"), (self.seq_fps - v).abs() < 1e-6).clicked() {
                            self.seq_fps = v;
                        }
                    }
                });
            });
        }
        let codec = SEQ_VIDEO_FORMATS[self.seq_format].2;
        let hint = formats::quality_hint(Some(codec), self.seq_quality);
        if hint != "lossless" {
            field(ui, "Quality", |ui| {
                ui.horizontal(|ui| {
                    let mut v = self.seq_quality / 100.0;
                    if theme::slider(ui, &mut v, (ui.available_width() - 110.0).max(120.0), 8.0, None).changed() {
                        self.seq_quality = (v * 100.0).round();
                    }
                    ui.label(RichText::new(format!("{:.0}%  {hint}", self.seq_quality)).size(12.5).color(p.muted));
                });
            });
        }
        if many {
            theme::toggle(ui, &mut self.seq_reverse, "Backwards (reverse order)");
            theme::toggle(ui, &mut self.seq_boomerang, "Boomerang (forward, then backward)");
            ui.add_space(4.0);
        }
        field(ui, "Save as", |ui| {
            ui.horizontal(|ui| {
                if theme::icon_button(ui, crate::icons::Icon::Folder, 30.0, false, "Choose file").clicked() {
                    let ext = SEQ_VIDEO_FORMATS[self.seq_format].1;
                    if let Some(f) = rfd::FileDialog::new().add_filter(ext, &[ext]).set_file_name(format!("video.{ext}")).save_file() {
                        self.seq_output = Some(f);
                    }
                }
                let label = self
                    .seq_output
                    .as_ref()
                    .map(|d| theme::truncate(&d.to_string_lossy(), 34))
                    .unwrap_or_else(|| "Next to the images".into());
                ui.label(RichText::new(label).size(12.5).color(p.muted));
            });
        });
        let frames = self.images.len() * if self.seq_boomerang && many { 2 } else { 1 };
        let seconds = if self.still_mode() { frames as f64 * self.seq_seconds } else { frames as f64 / self.seq_fps };
        theme::chip(ui, &format!("{frames} image{} → {} of video", if frames == 1 { "" } else { "s" }, util::fmt_time(seconds)), p.accent);
    }

    pub fn start_assemble(&mut self, queue: &mut Queue, images: &[PathBuf]) -> Result<String, String> {
        self.sync_images(images);
        let job = self.assemble_job()?;
        let msg = format!("Creating {}", job.output.display());
        queue.push(job);
        Ok(msg)
    }

    fn range(&self) -> (f64, Option<f64>) {
        let start = util::parse_time(&self.start).unwrap_or(0.0);
        (start, util::parse_time(&self.end).filter(|e| *e > start))
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
        // A single picture (or "seconds per image" mode) becomes a still/slideshow video.
        let still = self.still_mode();
        let animated = matches!(codec, VCodec::Gif | VCodec::WebpAnim);
        let out_fps = if !still { self.seq_fps } else if animated { 10.0 } else { 30.0 };

        // ffconcat list: works with any file names, orders and mixed formats.
        let list = std::env::temp_dir().join(format!("vmpc_seq_{}_{}.ffconcat", std::process::id(), rand_suffix()));
        let mut file = std::fs::File::create(&list).map_err(|e| e.to_string())?;
        let dur = if still { self.seq_seconds.max(0.1) } else { 1.0 / self.seq_fps };
        let mut text = String::from("ffconcat version 1.0\n");
        for f in &frames {
            let path = f.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
            text.push_str(&format!("file '{path}'\nduration {dur:.6}\n"));
        }
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;

        let output = match &self.seq_output {
            Some(o) => o.with_extension(ext),
            None => {
                let dir = first.parent().map(PathBuf::from).unwrap_or_else(|| util::default_output_dir("Videos"));
                if self.images.len() == 1 {
                    // One picture: save next to it, named after it.
                    let stem = first.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
                    util::unique_path(&dir, &stem, ext)
                } else {
                    let stem = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "sequence".into());
                    let stem = if self.seq_reverse { format!("{stem}_reversed") } else { stem };
                    let parent = dir.parent().map(PathBuf::from).unwrap_or(dir.clone());
                    util::unique_path(&parent, &stem, ext)
                }
            }
        };
        // Timing: the list gives each image its duration; tpad holds the last image for
        // its full time, fps turns it into a constant frame rate and -t (below) trims to
        // exactly images × duration.
        let mut vf = vec![
            format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"),
            format!("tpad=stop_mode=clone:stop_duration={dur:.6}"),
            format!("fps={out_fps:.6}"),
        ];
        if codec == VCodec::Gif {
            vf.push(formats::GIF_PALETTE.to_string());
        }
        let mut args: Vec<OsString> = ["-f", "concat", "-safe", "0", "-i"].iter().map(OsString::from).collect();
        args.push(list.clone().into_os_string());
        args.push("-vf".into());
        args.push(vf.join(",").into());
        for a in formats::video_args(codec, self.seq_quality) {
            args.push(a.into());
        }
        if ext == "mp4" {
            args.extend(["-movflags", "+faststart"].map(OsString::from));
        }
        let total_secs = frames.len() as f64 * dur;
        args.extend(["-t".into(), format!("{total_secs:.3}").into()]);
        args.push(output.clone().into_os_string());
        let total = Some(total_secs);
        let title = if still {
            format!("{} image{} → {name}, {}s each", frames.len(), if frames.len() == 1 { "" } else { "s" }, self.seq_seconds)
        } else {
            format!("{} images → {name} @ {} fps", frames.len(), self.seq_fps)
        };
        let mut job = Job::new(title, output, args, total);
        job.temp.push(list);
        Ok(job)
    }
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

#[cfg(test)]
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

    #[test]
    fn single_image_and_slideshow_become_videos() {
        let Some(sample) = testing::sample_video() else { return };
        let out = testing::scratch_dir("stills");
        let png = out.join("cover.png");
        let ok = ffmpeg::ffmpeg_cmd().unwrap().args(["-y", "-v", "error", "-i"]).arg(&sample).args(["-frames:v", "1"]).arg(&png).status().unwrap();
        assert!(ok.success());
        let png2 = out.join("cover2.png");
        std::fs::copy(&png, &png2).unwrap();
        let duration = |job: Job| {
            let job = testing::run(job);
            assert_eq!(job.status, Status::Done, "{:?}", job.status);
            ffmpeg::probe(&job.output.to_string_lossy()).unwrap().duration.unwrap_or(0.0)
        };
        // One picture → a 2 s video (no fps needed).
        let single = Frames { images: vec![png.clone()], seq_seconds: 2.0, seq_output: Some(out.join("single")), ..Default::default() };
        let d = duration(single.assemble_job().unwrap());
        assert!((d - 2.0).abs() < 0.15, "single image video lasted {d}");
        // Two pictures in "seconds per image" mode → 2 × 1.5 s slideshow.
        let show = Frames {
            images: vec![png, png2],
            seq_still: true,
            seq_seconds: 1.5,
            seq_output: Some(out.join("show")),
            ..Default::default()
        };
        let d = duration(show.assemble_job().unwrap());
        assert!((d - 3.0).abs() < 0.15, "slideshow lasted {d}");
        let _ = std::fs::remove_dir_all(&out);
    }
}
