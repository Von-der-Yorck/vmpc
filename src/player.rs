//! Playback engine.
//!
//! * Video: an `ffmpeg` child decodes, resamples to a constant frame rate and scales
//!   to the display size, then streams raw RGBA frames over a pipe. A reader thread
//!   turns them into `ColorImage`s and hands them over a small bounded channel, so
//!   decoding naturally pauses when the UI is not consuming frames.
//! * Audio: a second `ffmpeg` child resamples to the output device format (f32,
//!   device rate / channel count, optional `atempo` for speed) and a feeder thread
//!   fills a ring buffer that the `cpal` callback drains.
//! * Sync: the audio device is the master clock (frames actually played); without
//!   audio we fall back to a wall clock. Video frames are shown when their PTS is due.

use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::ffmpeg::{self, MediaInfo};
use crate::util;

// ---------------------------------------------------------------------------
// Audio output (one persistent cpal stream for the whole app)
// ---------------------------------------------------------------------------

struct AudioShared {
    buf: Mutex<VecDeque<f32>>,
    played_frames: AtomicU64,
    generation: AtomicU64,
    paused: AtomicBool,
    feeder_done: AtomicBool,
    volume: AtomicU32,
    level: AtomicU32,
}

struct AudioOut {
    _stream: cpal::Stream,
    shared: Arc<AudioShared>,
    rate: u32,
    channels: u16,
}

impl AudioOut {
    fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("No audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let shared = Arc::new(AudioShared {
            buf: Mutex::new(VecDeque::with_capacity(1 << 17)),
            played_frames: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            paused: AtomicBool::new(true),
            feeder_done: AtomicBool::new(true),
            volume: AtomicU32::new(1.0f32.to_bits()),
            level: AtomicU32::new(0),
        });
        let stream = match format {
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, shared.clone()),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, shared.clone()),
            cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config, shared.clone()),
            _ => build_stream::<f32>(&device, &config, shared.clone()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, shared, rate: config.sample_rate.0, channels: config.channels })
    }
}

fn build_stream<T>(device: &cpal::Device, config: &cpal::StreamConfig, shared: Arc<AudioShared>) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let silence = T::from_sample(0.0f32);
                if shared.paused.load(Ordering::Relaxed) {
                    data.fill(silence);
                    shared.level.store(0, Ordering::Relaxed);
                    return;
                }
                let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                let mut buf = shared.buf.lock().unwrap();
                let frames = (buf.len() / channels).min(data.len() / channels);
                let mut energy = 0.0f32;
                for (out, sample) in data.iter_mut().zip(buf.drain(..frames * channels)) {
                    let v = (sample * volume).clamp(-1.0, 1.0);
                    energy += v * v;
                    *out = T::from_sample(v);
                }
                drop(buf);
                data[frames * channels..].fill(silence);
                shared.played_frames.fetch_add(frames as u64, Ordering::Relaxed);
                let rms = if frames > 0 { (energy / (frames * channels) as f32).sqrt() } else { 0.0 };
                shared.level.store(rms.to_bits(), Ordering::Relaxed);
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Decoder children
// ---------------------------------------------------------------------------

/// Kills its ffmpeg child when dropped, which in turn ends the reader thread.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Frame {
    pts: f64,
    image: ColorImage,
}

struct VideoStream {
    rx: Receiver<Frame>,
    done: Arc<AtomicBool>,
    _child: ChildGuard,
}

fn atempo_chain(speed: f64) -> String {
    // atempo accepts 0.5..=2.0 per instance; chain for anything outside.
    let mut parts = Vec::new();
    let mut s = speed;
    while s > 2.0 {
        parts.push("atempo=2.0".to_string());
        s /= 2.0;
    }
    while s < 0.5 {
        parts.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    parts.push(format!("atempo={s:.4}"));
    parts.join(",")
}

/// Views a pixel buffer as raw bytes. `Color32` is `#[repr(C)]` `[u8; 4]` (RGBA,
/// alignment 1), so this is the exact `-pix_fmt rgba` layout FFmpeg writes. Video
/// frames are opaque, so premultiplied and straight alpha are identical.
fn as_bytes_mut(pixels: &mut [egui::Color32]) -> &mut [u8] {
    const _: () = assert!(std::mem::size_of::<egui::Color32>() == 4 && std::mem::align_of::<egui::Color32>() == 1);
    // SAFETY: size/alignment checked above; every byte pattern is a valid Color32.
    unsafe { std::slice::from_raw_parts_mut(pixels.as_mut_ptr().cast::<u8>(), pixels.len() * 4) }
}

#[derive(Clone, Copy)]
struct VideoParams {
    width: u32,
    height: u32,
    fps: f64,
    still: bool,
    hwaccel: bool,
}

fn spawn_video(source: &str, start: f64, p: VideoParams) -> Result<VideoStream, String> {
    let mut cmd = ffmpeg::ffmpeg_cmd().ok_or("FFmpeg not found")?;
    cmd.args(["-v", "error"]);
    if p.hwaccel && !p.still {
        cmd.args(["-hwaccel", "auto"]);
    }
    if start > 0.0 {
        cmd.args(["-ss", &format!("{start:.3}")]);
    }
    cmd.arg("-i").arg(source);
    let (w, h) = (p.width, p.height);
    let scale = format!("scale={w}:{h}:flags=bilinear");
    let vf = if p.still { scale } else { format!("fps={:.5},{scale}", p.fps) };
    cmd.args(["-map", "0:V:0?", "-an", "-sn", "-dn", "-vf", &vf, "-pix_fmt", "rgba", "-f", "rawvideo"]);
    if p.still {
        cmd.args(["-frames:v", "1"]);
    }
    cmd.arg("-");
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start ffmpeg: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx): (SyncSender<Frame>, _) = mpsc::sync_channel(3);
    let done = Arc::new(AtomicBool::new(false));
    let done2 = done.clone();
    let fps = p.fps;
    std::thread::Builder::new()
        .name("video-reader".into())
        .spawn(move || {
            let pixel_count = (w * h) as usize;
            let mut index = 0u64;
            loop {
                // Decode straight into the pixel buffer that becomes the texture: no copies.
                let mut pixels = vec![egui::Color32::TRANSPARENT; pixel_count];
                if stdout.read_exact(as_bytes_mut(&mut pixels)).is_err() {
                    break;
                }
                let image = ColorImage { size: [w as usize, h as usize], pixels };
                let pts = start + index as f64 / fps;
                if tx.send(Frame { pts, image }).is_err() {
                    break;
                }
                index += 1;
            }
            done2.store(true, Ordering::Release);
        })
        .map_err(|e| e.to_string())?;
    Ok(VideoStream { rx, done, _child: ChildGuard(child) })
}

fn spawn_audio(source: &str, start: f64, speed: f64, out: &AudioOut) -> Result<ChildGuard, String> {
    let shared = &out.shared;
    let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
    shared.buf.lock().unwrap().clear();
    shared.played_frames.store(0, Ordering::SeqCst);
    shared.feeder_done.store(false, Ordering::SeqCst);

    let mut cmd = ffmpeg::ffmpeg_cmd().ok_or("FFmpeg not found")?;
    cmd.args(["-v", "error"]);
    if start > 0.0 {
        cmd.args(["-ss", &format!("{start:.3}")]);
    }
    cmd.arg("-i").arg(source);
    cmd.args(["-map", "0:a:0", "-vn", "-sn", "-dn"]);
    if (speed - 1.0).abs() > 1e-3 {
        cmd.args(["-af", &atempo_chain(speed)]);
    }
    cmd.args(["-ac", &out.channels.to_string(), "-ar", &out.rate.to_string(), "-f", "f32le", "-"]);
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start ffmpeg: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let shared = shared.clone();
    // Keep about half a second buffered ahead of the device.
    let high_water = (out.rate as usize * out.channels as usize) / 2;
    std::thread::Builder::new()
        .name("audio-feeder".into())
        .spawn(move || {
            let mut raw = vec![0u8; 16 * 1024];
            let mut carry: Vec<u8> = Vec::with_capacity(4);
            'outer: loop {
                let n = match stdout.read(&mut raw) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                carry.extend_from_slice(&raw[..n]);
                let usable = carry.len() / 4 * 4;
                let samples: Vec<f32> = carry[..usable]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                carry.drain(..usable);
                loop {
                    if shared.generation.load(Ordering::SeqCst) != generation {
                        break 'outer;
                    }
                    let mut buf = shared.buf.lock().unwrap();
                    if buf.len() < high_water {
                        buf.extend(samples.iter());
                        break;
                    }
                    drop(buf);
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            if shared.generation.load(Ordering::SeqCst) == generation {
                shared.feeder_done.store(true, Ordering::SeqCst);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(ChildGuard(child))
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

#[derive(Default)]
struct WallClock {
    base: f64,
    anchor: Option<Instant>,
    speed: f64,
}

impl WallClock {
    fn now(&self) -> f64 {
        self.base + self.anchor.map(|a| a.elapsed().as_secs_f64() * self.speed).unwrap_or(0.0)
    }
    fn set(&mut self, t: f64, running: bool) {
        self.base = t;
        self.anchor = running.then(Instant::now);
    }
}

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MaxRes {
    P720,
    P1080,
    P1440,
    P2160,
}

impl MaxRes {
    pub const ALL: [MaxRes; 4] = [MaxRes::P720, MaxRes::P1080, MaxRes::P1440, MaxRes::P2160];
    pub fn size(self) -> (u32, u32) {
        match self {
            MaxRes::P720 => (1280, 720),
            MaxRes::P1080 => (1920, 1080),
            MaxRes::P1440 => (2560, 1440),
            MaxRes::P2160 => (3840, 2160),
        }
    }
    pub fn height(self) -> u32 {
        self.size().1
    }
    pub fn from_height(h: u32) -> Self {
        Self::ALL.into_iter().find(|r| r.height() >= h).unwrap_or(MaxRes::P2160)
    }
    pub fn label(self) -> &'static str {
        match self {
            MaxRes::P720 => "720p (lightest)",
            MaxRes::P1080 => "1080p",
            MaxRes::P1440 => "1440p",
            MaxRes::P2160 => "4K",
        }
    }
}

pub enum LoadState {
    Idle,
    Loading,
    Ready,
    Failed(String),
}

pub struct Player {
    pub source: Option<String>,
    pub info: Option<MediaInfo>,
    pub state: LoadState,
    pub texture: Option<TextureHandle>,
    pub cover: Option<TextureHandle>,
    pub paused: bool,
    pub volume: f32,
    pub muted: bool,
    pub speed: f64,
    pub max_res: MaxRes,
    pub hwaccel: bool,
    pub ended: bool,
    ended_event: bool,

    audio_out: Option<AudioOut>,
    audio_error: Option<String>,
    audio_child: Option<ChildGuard>,
    audio_start: f64,
    video: Option<VideoStream>,
    video_params: Option<VideoParams>,
    pending: Option<Frame>,
    force_next_frame: bool,
    last_pts: f64,
    last_position: f64,
    audio_seen: (u64, Instant),
    lag_since: Option<Instant>,
    last_resync: Option<Instant>,
    clock: WallClock,
    probe_rx: Option<Receiver<(String, Result<MediaInfo, String>)>>,
    cover_rx: Option<Receiver<Option<ColorImage>>>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            source: None,
            info: None,
            state: LoadState::Idle,
            texture: None,
            cover: None,
            paused: true,
            volume: 0.8,
            muted: false,
            speed: 1.0,
            max_res: MaxRes::P1080,
            hwaccel: true,
            ended: false,
            ended_event: false,
            audio_out: None,
            audio_error: None,
            audio_child: None,
            audio_start: 0.0,
            video: None,
            video_params: None,
            pending: None,
            force_next_frame: false,
            last_pts: 0.0,
            last_position: 0.0,
            audio_seen: (0, Instant::now()),
            lag_since: None,
            last_resync: None,
            clock: WallClock { speed: 1.0, ..Default::default() },
            probe_rx: None,
            cover_rx: None,
        }
    }
}

impl Player {
    /// Starts loading a file path or URL. Probing runs in the background.
    pub fn open(&mut self, source: String) {
        self.stop();
        self.source = Some(source.clone());
        self.state = LoadState::Loading;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let res = ffmpeg::probe(&source);
            let _ = tx.send((source, res));
        });
        self.probe_rx = Some(rx);
    }

    /// Stops playback and releases all decoders.
    pub fn stop(&mut self) {
        self.video = None;
        self.audio_child = None;
        if let Some(out) = &self.audio_out {
            out.shared.generation.fetch_add(1, Ordering::SeqCst);
            out.shared.paused.store(true, Ordering::SeqCst);
            out.shared.buf.lock().unwrap().clear();
        }
        self.pending = None;
        self.info = None;
        self.source = None;
        self.texture = None;
        self.cover = None;
        self.cover_rx = None;
        self.probe_rx = None;
        self.state = LoadState::Idle;
        self.paused = true;
        self.ended = false;
        self.clock.set(0.0, false);
    }

    /// True exactly once after playback reached the end.
    pub fn take_ended_event(&mut self) -> bool {
        std::mem::take(&mut self.ended_event)
    }

    pub fn is_loaded(&self) -> bool {
        matches!(self.state, LoadState::Ready)
    }

    pub fn is_still(&self) -> bool {
        self.info.as_ref().is_some_and(|i| i.is_still())
            && !self.source.as_deref().is_some_and(util::is_url)
    }

    pub fn duration(&self) -> Option<f64> {
        self.info.as_ref().and_then(|i| i.duration)
    }

    pub fn has_audio(&self) -> bool {
        self.info.as_ref().is_some_and(|i| i.audio.is_some())
    }

    pub fn has_video(&self) -> bool {
        self.info.as_ref().is_some_and(|i| i.video.is_some())
    }

    /// Smoothed output level 0..1 for the audio visualiser.
    pub fn level(&self) -> f32 {
        self.audio_out
            .as_ref()
            .map(|o| f32::from_bits(o.shared.level.load(Ordering::Relaxed)))
            .unwrap_or(0.0)
    }

    pub fn audio_error(&self) -> Option<&str> {
        self.audio_error.as_deref()
    }

    fn audio_active(&self) -> bool {
        self.audio_child.is_some() && self.audio_out.is_some()
    }

    fn audio_exhausted(&self) -> bool {
        match &self.audio_out {
            Some(out) if self.audio_child.is_some() => {
                out.shared.feeder_done.load(Ordering::SeqCst) && out.shared.buf.lock().unwrap().is_empty()
            }
            _ => true,
        }
    }

    /// Current media position in seconds.
    pub fn position(&mut self) -> f64 {
        let mut t = if self.audio_active() && !self.audio_exhausted() {
            let out = self.audio_out.as_ref().unwrap();
            let played = out.shared.played_frames.load(Ordering::Relaxed);
            // The device consumes audio in chunks (10-20 ms); interpolate between them so
            // the clock is smooth and the UI wakes exactly when a frame is due.
            if played != self.audio_seen.0 {
                self.audio_seen = (played, Instant::now());
            }
            let since = if self.paused || played == 0 { 0.0 } else { self.audio_seen.1.elapsed().as_secs_f64().min(0.1) };
            let t = self.audio_start + (played as f64 / out.rate as f64 + since) * self.speed;
            let t = t.max(self.last_position);
            self.last_position = t;
            // Keep the wall clock aligned so it can take over seamlessly when audio ends.
            self.clock.set(t, !self.paused);
            t
        } else {
            self.clock.now()
        };
        if let Some(d) = self.duration() {
            t = t.min(d);
        }
        t.max(0.0)
    }

    fn sync_volume(&self) {
        if let Some(out) = &self.audio_out {
            let v = if self.muted { 0.0 } else { self.volume * self.volume };
            out.shared.volume.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    fn start_pipelines(&mut self, at: f64) {
        let Some(source) = self.source.clone() else { return };
        self.pending = None;
        self.last_pts = at;
        self.last_position = at;
        self.audio_seen = (0, Instant::now());
        self.ended = false;
        self.video = None;
        self.audio_child = None;
        self.video = match self.video_params {
            Some(p) => match spawn_video(&source, at, p) {
                Ok(v) => Some(v),
                Err(e) => {
                    self.state = LoadState::Failed(e);
                    None
                }
            },
            None => None,
        };
        if self.has_audio() && !self.is_still() {
            if self.audio_out.is_none() && self.audio_error.is_none() {
                match AudioOut::new() {
                    Ok(out) => self.audio_out = Some(out),
                    Err(e) => self.audio_error = Some(e),
                }
            }
            if let Some(out) = &self.audio_out {
                self.audio_child = spawn_audio(&source, at, self.speed, out).ok();
                out.shared.paused.store(self.paused, Ordering::SeqCst);
            }
        }
        self.sync_volume();
        self.audio_start = at;
        self.clock.speed = self.speed;
        self.clock.set(at, !self.paused);
    }

    fn resync_video(&mut self, at: f64) {
        let (Some(source), Some(params)) = (self.source.clone(), self.video_params) else { return };
        self.video = None;
        self.video = spawn_video(&source, at, params).ok();
        self.pending = None;
        self.last_pts = at;
        self.lag_since = None;
        self.last_resync = Some(Instant::now());
    }

    pub fn seek(&mut self, t: f64) {
        if !self.is_loaded() || self.is_still() {
            return;
        }
        let t = match self.duration() {
            Some(d) => t.clamp(0.0, (d - 0.05).max(0.0)),
            None => t.max(0.0),
        };
        self.force_next_frame = true;
        self.start_pipelines(t);
    }

    pub fn set_paused(&mut self, paused: bool) {
        if !self.is_loaded() {
            return;
        }
        if !paused && self.ended {
            self.paused = false;
            self.seek(0.0);
            return;
        }
        let now = self.position();
        self.paused = paused;
        self.clock.set(now, !paused);
        if let Some(out) = &self.audio_out {
            out.shared.paused.store(paused || self.audio_child.is_none(), Ordering::SeqCst);
        }
    }

    pub fn toggle_pause(&mut self) {
        self.set_paused(!self.paused);
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        self.sync_volume();
    }

    pub fn set_muted(&mut self, m: bool) {
        self.muted = m;
        self.sync_volume();
    }

    pub fn set_speed(&mut self, speed: f64) {
        let speed = speed.clamp(0.25, 4.0);
        if (speed - self.speed).abs() < 1e-6 {
            return;
        }
        let now = self.position();
        self.speed = speed;
        if self.is_loaded() && !self.is_still() {
            self.start_pipelines(now);
        }
    }

    /// Step one frame forward (pauses playback).
    pub fn step_frame(&mut self) {
        let fps = self.info.as_ref().and_then(|i| i.video.as_ref()).map(|v| v.fps).unwrap_or(25.0);
        if !self.paused {
            self.set_paused(true);
        }
        let t = self.position() + 1.0 / fps.min(60.0);
        self.seek(t);
    }

    /// Re-creates decoders after a quality setting changed.
    pub fn reload_settings(&mut self) {
        if self.is_loaded() {
            self.configure_video();
            let now = self.position();
            self.force_next_frame = true;
            self.start_pipelines(now);
        }
    }

    fn configure_video(&mut self) {
        let Some(info) = &self.info else { return };
        self.video_params = info.video.as_ref().map(|v| {
            let (mw, mh) = self.max_res.size();
            let (width, height) = ffmpeg::fit_size(v.width, v.height, mw, mh);
            VideoParams { width, height, fps: v.fps.clamp(1.0, 60.0), still: info.is_still(), hwaccel: self.hwaccel }
        });
    }

    fn upload(&mut self, ctx: &egui::Context, image: ColorImage) {
        match &mut self.texture {
            Some(tex) if tex.size() == image.size => tex.set(image, TextureOptions::LINEAR),
            _ => self.texture = Some(ctx.load_texture("vmpc-video", image, TextureOptions::LINEAR)),
        }
    }

    /// Drives the engine; call once per UI frame. Returns when the UI should repaint next.
    pub fn update(&mut self, ctx: &egui::Context) -> Option<Duration> {
        // Probe finished?
        if let Some(rx) = &self.probe_rx {
            match rx.try_recv() {
                Ok((source, result)) => {
                    self.probe_rx = None;
                    if self.source.as_deref() == Some(source.as_str()) {
                        match result {
                            Ok(info) => {
                                if info.video.is_none() && info.has_cover {
                                    let (tx, rx) = mpsc::channel();
                                    let src = source.clone();
                                    std::thread::spawn(move || {
                                        let _ = tx.send(ffmpeg::grab_frame(&src, 0.0, true, 512, 512));
                                    });
                                    self.cover_rx = Some(rx);
                                }
                                self.info = Some(info);
                                self.state = LoadState::Ready;
                                self.configure_video();
                                self.paused = false;
                                self.force_next_frame = true;
                                self.start_pipelines(0.0);
                            }
                            Err(e) => self.state = LoadState::Failed(e),
                        }
                    }
                }
                Err(TryRecvError::Empty) => return Some(Duration::from_millis(50)),
                Err(TryRecvError::Disconnected) => self.probe_rx = None,
            }
        }
        if let Some(rx) = &self.cover_rx {
            if let Ok(img) = rx.try_recv() {
                self.cover = img.map(|i| ctx.load_texture("vmpc-cover", i, TextureOptions::LINEAR));
                self.cover_rx = None;
            }
        }
        if !self.is_loaded() {
            return None;
        }

        let now = self.position();
        // egui wakes us one predicted frame early (it schedules for presentation), so a frame
        // due within that window goes up now; otherwise we would repaint at the full
        // display rate instead of the video rate.
        let interval = self.video_params.map(|p| 1.0 / p.fps).unwrap_or(1.0 / 30.0);
        let early = (ctx.input(|i| i.predicted_dt) as f64).min(interval * 0.5);
        let mut next_due: Option<f64> = None;
        let mut newest: Option<ColorImage> = None;
        if let Some(video) = &self.video {
            loop {
                if self.pending.is_none() {
                    match video.rx.try_recv() {
                        Ok(f) => self.pending = Some(f),
                        Err(_) => break,
                    }
                }
                let frame = self.pending.as_ref().unwrap();
                if self.force_next_frame || frame.pts <= now + early {
                    self.force_next_frame = false;
                    let shown = self.pending.take().unwrap();
                    self.last_pts = shown.pts;
                    newest = Some(shown.image);
                } else {
                    next_due = Some(frame.pts - now);
                    break;
                }
            }
        }
        if let Some(img) = newest {
            self.upload(ctx, img);
        }
        if self.video.is_none() {
            self.force_next_frame = false;
        }

        // End of media?
        let video_done = self
            .video
            .as_ref()
            .map(|v| v.done.load(Ordering::Acquire) && self.pending.is_none())
            .unwrap_or(true);
        // Decoder can't keep up (slow machine, huge video): rather than drifting out of
        // sync, jump the video pipe ahead to the audio clock, like a player dropping frames.
        let lagging = self.pending.is_none() && self.last_pts < now - 0.3;
        if !self.paused && lagging && !video_done && self.video.is_some() {
            let since = *self.lag_since.get_or_insert_with(Instant::now);
            let cooled_down = self.last_resync.map_or(true, |t| t.elapsed() > Duration::from_secs(5));
            if since.elapsed() > Duration::from_secs(1) && cooled_down {
                self.resync_video(now + 0.3);
            }
        } else {
            self.lag_since = None;
        }

        let audio_done = self.audio_exhausted();
        let past_end = self.duration().is_some_and(|d| now >= d - 0.02);
        if !self.paused && !self.is_still() && ((video_done && audio_done) || (past_end && video_done)) {
            self.set_paused(true);
            self.ended = true;
            self.ended_event = true;
        }

        if self.paused {
            return (self.force_next_frame).then(|| Duration::from_millis(15));
        }
        // No decoded frame waiting yet: wake up when the next one is expected, not in a busy loop.
        let expected = next_due.unwrap_or_else(|| (self.last_pts + interval - now).max(0.004));
        let wait = (expected / self.speed).clamp(0.002, 0.05);
        Some(Duration::from_secs_f64(wait))
    }

    /// Saves the current frame at full resolution next to `dir`. Runs in the background.
    pub fn snapshot(&mut self, dir: std::path::PathBuf) -> Option<std::path::PathBuf> {
        let source = self.source.clone()?;
        if !self.has_video() {
            return None;
        }
        let t = self.position();
        let stem = std::path::Path::new(&util::display_name(&source))
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "snapshot".into());
        let path = util::unique_path(&dir, &format!("{stem}_{}", util::fmt_time(t).replace(':', "-")), "png");
        let out = path.clone();
        std::thread::spawn(move || {
            if let Some(mut cmd) = ffmpeg::ffmpeg_cmd() {
                cmd.args(["-v", "error", "-ss", &format!("{t:.3}"), "-i"])
                    .arg(&source)
                    .args(["-map", "0:V:0", "-frames:v", "1", "-update", "1"])
                    .arg(&out)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _ = cmd.status();
            }
        });
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::atempo_chain;

    #[test]
    fn atempo_ranges() {
        assert_eq!(atempo_chain(1.5), "atempo=1.5000");
        assert_eq!(atempo_chain(4.0), "atempo=2.0,atempo=2.0000");
        assert_eq!(atempo_chain(0.25), "atempo=0.5,atempo=0.5000");
    }
}
