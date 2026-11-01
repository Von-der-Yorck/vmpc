//! FFmpeg backend: locating the binaries, spawning them without console windows,
//! probing media and grabbing single frames.
//!
//! VMPC drives the FFmpeg command line tools as subprocesses. That keeps the Rust
//! binary tiny, gives us every demuxer/decoder/encoder FFmpeg ships (the same codec
//! base VLC uses) and avoids fragile native linking on Windows and Linux.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub version: String,
}

/// `None` = not searched yet; `Some(None)` = searched, not found.
static TOOLS: Mutex<Option<Option<Tools>>> = Mutex::new(None);

/// The located FFmpeg tools, or `None` if FFmpeg is not installed.
pub fn tools() -> Option<Tools> {
    TOOLS.lock().unwrap().get_or_insert_with(locate).clone()
}

/// Forgets the cached location so the next [`tools`] call searches again.
pub fn refresh() {
    *TOOLS.lock().unwrap() = None;
}

/// Per-user folder where VMPC keeps an FFmpeg it downloaded itself.
pub fn managed_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("VMPC");
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(std::env::temp_dir)
        .join("vmpc");
    base.join("ffmpeg")
}

// ---------------------------------------------------------------------------
// One-click engine download (static GPL builds from github.com/BtbN/FFmpeg-Builds)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Download {
    Idle,
    Running,
    Done,
    Failed(String),
}

static DOWNLOAD: Mutex<Download> = Mutex::new(Download::Idle);

pub fn download_state() -> Download {
    DOWNLOAD.lock().unwrap().clone()
}

#[cfg(windows)]
const ENGINE_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
#[cfg(not(windows))]
const ENGINE_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz";

/// Downloads FFmpeg into [`managed_dir`] in the background using the system's
/// `curl` and `tar` (both built into Windows 10+ and every Linux distribution).
pub fn start_download(ctx: egui::Context) {
    {
        let mut state = DOWNLOAD.lock().unwrap();
        if *state == Download::Running {
            return;
        }
        *state = Download::Running;
    }
    std::thread::spawn(move || {
        let result = download_engine();
        refresh();
        *DOWNLOAD.lock().unwrap() = match result {
            Ok(()) if tools().is_some() => Download::Done,
            Ok(()) => Download::Failed("downloaded, but the engine does not start".into()),
            Err(e) => Download::Failed(e),
        };
        ctx.request_repaint();
    });
}

fn download_engine() -> Result<(), String> {
    let dir = managed_dir();
    let work = dir.with_file_name("ffmpeg-download");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let archive = work.join(ENGINE_URL.rsplit('/').next().unwrap_or("ffmpeg-archive"));
    let run = |cmd: &mut Command, what: &str| -> Result<(), String> {
        let out = cmd.stdout(Stdio::null()).stderr(Stdio::piped()).output().map_err(|e| format!("{what}: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("{what} failed: {}", err.lines().last().unwrap_or("").trim()))
        }
    };
    // On Windows use the built-in tools explicitly (bsdtar unpacks .zip; a GNU tar on PATH would not).
    #[cfg(windows)]
    let (curl, tar) = {
        let sys = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
        (sys.join("curl.exe"), sys.join("tar.exe"))
    };
    #[cfg(not(windows))]
    let (curl, tar) = (PathBuf::from("curl"), PathBuf::from("tar"));
    run(command(&curl).args(["-L", "--fail", "--silent", "--show-error", "-o"]).arg(&archive).arg(ENGINE_URL), "Download")?;
    run(command(&tar).arg("-xf").arg(&archive).arg("-C").arg(&work), "Unpacking")?;
    let found = find_file(&work, &exe("ffmpeg")).ok_or("ffmpeg not found in the download")?;
    let bin = found.parent().ok_or("bad archive layout")?;
    let target = dir.join("bin");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for name in [exe("ffmpeg"), exe("ffprobe")] {
        std::fs::rename(bin.join(&name), target.join(&name))
            .or_else(|_| std::fs::copy(bin.join(&name), target.join(&name)).map(|_| ()))
            .map_err(|e| format!("Installing {name}: {e}"))?;
    }
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if entry.file_name().to_string_lossy() == name {
            return Some(path);
        }
    }
    None
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(dir) = std::env::var("VMPC_FFMPEG_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
        dirs.push(exe_dir.join("ffmpeg").join("bin"));
        dirs.push(exe_dir.join("ffmpeg"));
        dirs.push(exe_dir);
    }
    dirs.push(managed_dir().join("bin"));
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    #[cfg(windows)]
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        // winget puts its aliases here; a freshly installed FFmpeg may not be on PATH yet.
        let local = PathBuf::from(local);
        dirs.push(local.join("Microsoft").join("WinGet").join("Links"));
        if let Ok(entries) = std::fs::read_dir(local.join("Microsoft").join("WinGet").join("Packages")) {
            for pkg in entries.flatten().filter(|e| e.file_name().to_string_lossy().contains("FFmpeg")) {
                if let Ok(inner) = std::fs::read_dir(pkg.path()) {
                    dirs.extend(inner.flatten().map(|e| e.path().join("bin")));
                }
            }
        }
    }
    #[cfg(not(windows))]
    dirs.extend(["/usr/bin", "/usr/local/bin", "/snap/bin"].map(PathBuf::from));
    dirs
}

fn locate() -> Option<Tools> {
    for dir in candidate_dirs() {
        let ffmpeg = dir.join(exe("ffmpeg"));
        let ffprobe = dir.join(exe("ffprobe"));
        if !(ffmpeg.is_file() && ffprobe.is_file()) {
            continue;
        }
        let Ok(out) = command(&ffmpeg).arg("-version").output() else { continue };
        if !out.status.success() {
            continue;
        }
        let version = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .split(" Copyright")
            .next()
            .unwrap_or_default()
            .to_string();
        return Some(Tools { ffmpeg, ffprobe, version });
    }
    None
}

/// A `Command` that never pops up a console window on Windows.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null());
    cmd
}

pub fn ffmpeg_cmd() -> Option<Command> {
    tools().map(|t| {
        let mut c = command(&t.ffmpeg);
        c.args(["-hide_banner", "-nostdin"]);
        c
    })
}

// ---------------------------------------------------------------------------
// Probing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct MediaInfo {
    pub duration: Option<f64>,
    pub format_name: String,
    pub bit_rate: Option<u64>,
    pub video: Option<VideoInfo>,
    pub audio: Option<AudioInfo>,
    /// Embedded cover art (e.g. album art in MP3/M4A/FLAC).
    pub has_cover: bool,
}

#[derive(Clone, Debug)]
pub struct VideoInfo {
    pub codec: String,
    /// Display size: sample aspect ratio and rotation already applied.
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frames: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct AudioInfo {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
}

/// What a file is, as far as the UI is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Video => "VIDEO",
            MediaKind::Audio => "AUDIO",
            MediaKind::Image => "IMAGE",
        }
    }
}

impl MediaInfo {
    /// Detects the media type: still pictures are images (animated GIF/WebP count as
    /// video), anything with moving pictures is video, the rest is audio.
    pub fn kind(&self) -> MediaKind {
        if self.is_still() {
            MediaKind::Image
        } else if self.video.is_some() {
            MediaKind::Video
        } else {
            MediaKind::Audio
        }
    }

    /// A still image (a single picture, no timeline).
    pub fn is_still(&self) -> bool {
        self.audio.is_none()
            && self.video.as_ref().is_some_and(|v| v.frames.unwrap_or(0) <= 1)
            && self.duration.unwrap_or(0.0) < 0.1
    }

    pub fn summary(&self) -> String {
        let mut parts = vec![self.format_name.clone()];
        if let Some(v) = &self.video {
            parts.push(format!("{} {}×{} @ {:.3} fps", v.codec, v.width, v.height, v.fps).replace(".000 fps", " fps"));
        }
        if let Some(a) = &self.audio {
            parts.push(format!("{} {} Hz {}ch", a.codec, a.sample_rate, a.channels));
        }
        if let Some(b) = self.bit_rate {
            parts.push(format!("{} kb/s", b / 1000));
        }
        parts.join("  ·  ")
    }
}

#[derive(Deserialize)]
struct ProbeOut {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
    format_long_name: Option<String>,
    format_name: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Deserialize, Default)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    sample_aspect_ratio: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    nb_frames: Option<String>,
    duration: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    #[serde(default)]
    disposition: std::collections::HashMap<String, i64>,
    #[serde(default)]
    tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    side_data_list: Vec<serde_json::Value>,
}

fn ratio(s: &Option<String>) -> Option<f64> {
    let s = s.as_deref()?;
    let (n, d) = s.split_once(['/', ':']).unwrap_or((s, "1"));
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (d != 0.0 && n > 0.0).then(|| n / d)
}

/// Runs ffprobe on a file path or URL.
pub fn probe(source: &str) -> Result<MediaInfo, String> {
    let tools = tools().ok_or("FFmpeg was not found")?;
    let out = command(&tools.ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", "-i"])
        .arg(source)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Could not start ffprobe: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().last().unwrap_or("unknown error").trim();
        return Err(format!("Unsupported or unreadable media: {line}"));
    }
    let parsed: ProbeOut =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("Could not read probe output: {e}"))?;

    let mut info = MediaInfo::default();
    if let Some(f) = &parsed.format {
        info.duration = f.duration.as_deref().and_then(|d| d.parse().ok()).filter(|d: &f64| *d > 0.0);
        info.format_name = f.format_long_name.clone().or_else(|| f.format_name.clone()).unwrap_or_default();
        info.bit_rate = f.bit_rate.as_deref().and_then(|b| b.parse().ok());
    }

    for s in &parsed.streams {
        match s.codec_type.as_deref() {
            Some("video") => {
                let attached = s.disposition.get("attached_pic").copied().unwrap_or(0) == 1;
                if attached {
                    info.has_cover = true;
                    continue;
                }
                if info.video.is_some() {
                    continue;
                }
                let (Some(mut w), Some(mut h)) = (s.width, s.height) else { continue };
                if let Some(sar) = ratio(&s.sample_aspect_ratio) {
                    if (sar - 1.0).abs() > 0.01 {
                        w = (w as f64 * sar).round() as u32;
                    }
                }
                let rotation = s
                    .side_data_list
                    .iter()
                    .find_map(|d| d.get("rotation").and_then(|r| r.as_f64()))
                    .or_else(|| s.tags.get("rotate").and_then(|r| r.parse().ok()))
                    .unwrap_or(0.0);
                if ((rotation.abs() as i64) % 180) == 90 {
                    std::mem::swap(&mut w, &mut h);
                }
                let fps = ratio(&s.avg_frame_rate)
                    .filter(|f| *f < 1000.0)
                    .or_else(|| ratio(&s.r_frame_rate).filter(|f| *f < 1000.0))
                    .unwrap_or(25.0);
                let frames = s.nb_frames.as_deref().and_then(|n| n.parse().ok());
                if info.duration.is_none() {
                    info.duration = s.duration.as_deref().and_then(|d| d.parse().ok());
                }
                info.video = Some(VideoInfo {
                    codec: s.codec_name.clone().unwrap_or_default(),
                    width: w.max(1),
                    height: h.max(1),
                    fps,
                    frames,
                });
            }
            Some("audio") if info.audio.is_none() => {
                info.audio = Some(AudioInfo {
                    codec: s.codec_name.clone().unwrap_or_default(),
                    sample_rate: s.sample_rate.as_deref().and_then(|r| r.parse().ok()).unwrap_or(48_000),
                    channels: s.channels.unwrap_or(2),
                });
            }
            _ => {}
        }
    }
    if info.video.is_none() && info.audio.is_none() {
        return Err("No playable audio or video stream found".into());
    }
    Ok(info)
}

/// Fits `(w, h)` inside `(max_w, max_h)` keeping aspect ratio; result has even dimensions.
pub fn fit_size(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let scale = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    let even = |v: f64| ((v.round() as u32) / 2 * 2).max(2);
    (even(w as f64 * scale), even(h as f64 * scale))
}

/// Decodes one frame (at `at` seconds, or the cover art) to RGBA.
pub fn grab_frame(source: &str, at: f64, cover: bool, w: u32, h: u32) -> Option<egui::ColorImage> {
    let mut cmd = ffmpeg_cmd()?;
    cmd.args(["-v", "error"]);
    if at > 0.0 {
        cmd.args(["-ss", &format!("{at:.3}")]);
    }
    cmd.arg("-i").arg(source);
    if cover {
        cmd.args(["-map", "0:v:0"]);
    }
    let vf = format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0");
    cmd.args(["-frames:v", "1", "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "rgba", "-"]);
    let out = cmd.stdout(Stdio::piped()).stderr(Stdio::null()).output().ok()?;
    let need = (w * h * 4) as usize;
    (out.stdout.len() >= need).then(|| egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &out.stdout[..need]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_and_even() {
        assert_eq!(fit_size(3840, 2160, 1920, 1080), (1920, 1080));
        assert_eq!(fit_size(1000, 3000, 1920, 1080), (360, 1080));
        assert_eq!(fit_size(641, 481, 1920, 1080), (640, 480));
    }

    #[test]
    fn ratios() {
        assert_eq!(ratio(&Some("30000/1001".into())).map(|r| (r * 100.0).round()), Some(2997.0));
        assert_eq!(ratio(&Some("0/0".into())), None);
        assert_eq!(ratio(&Some("4:3".into())).map(|r| (r * 3.0).round()), Some(4.0));
    }
}

#[cfg(test)]
mod download_tests {
    /// Real network download (~100 MB); run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn engine_download_installs_working_binaries() {
        super::download_engine().unwrap();
        let bin = super::managed_dir().join("bin");
        for name in ["ffmpeg", "ffprobe"] {
            let out = super::command(bin.join(super::exe(name))).arg("-version").output().unwrap();
            assert!(out.status.success(), "{name} does not run");
        }
    }
}
