//! Output format catalogue and the mapping from simple UI settings (quality 0-100,
//! resolution, bitrate) to FFmpeg encoder arguments.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Video,
    Audio,
    Image,
    Custom,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VCodec {
    H264,
    H265,
    Vp9,
    Av1,
    ProRes,
    ProRes4444,
    Mpeg4,
    Mpeg2,
    Wmv,
    Ffv1,
    Gif,
    WebpAnim,
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ACodec {
    Mp3,
    Aac,
    Opus,
    Vorbis,
    Flac,
    Wav,
    Alac,
    Aiff,
    Wma,
    Mp2,
}

#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub name: &'static str,
    pub ext: &'static str,
    pub kind: Kind,
    pub video: Option<VCodec>,
    pub audio: Option<ACodec>,
}

const fn f(name: &'static str, ext: &'static str, kind: Kind, video: Option<VCodec>, audio: Option<ACodec>) -> Format {
    Format { name, ext, kind, video, audio }
}

use ACodec as A;
use Kind::*;
use VCodec as V;

pub const FORMATS: &[Format] = &[
    f("MP4 · H.264", "mp4", Video, Some(V::H264), Some(A::Aac)),
    f("MP4 · H.265", "mp4", Video, Some(V::H265), Some(A::Aac)),
    f("MKV · H.264", "mkv", Video, Some(V::H264), Some(A::Aac)),
    f("WebM · VP9", "webm", Video, Some(V::Vp9), Some(A::Opus)),
    f("MKV · AV1", "mkv", Video, Some(V::Av1), Some(A::Opus)),
    f("MOV · ProRes", "mov", Video, Some(V::ProRes), Some(A::Wav)),
    f("AVI · MPEG-4", "avi", Video, Some(V::Mpeg4), Some(A::Mp3)),
    f("MPG · MPEG-2", "mpg", Video, Some(V::Mpeg2), Some(A::Mp2)),
    f("WMV", "wmv", Video, Some(V::Wmv), Some(A::Wma)),
    f("TS · H.264", "ts", Video, Some(V::H264), Some(A::Aac)),
    f("MKV · FFV1 lossless", "mkv", Video, Some(V::Ffv1), Some(A::Flac)),
    f("GIF · animated", "gif", Video, Some(V::Gif), None),
    f("WebP · animated", "webp", Video, Some(V::WebpAnim), None),
    f("MP3", "mp3", Audio, None, Some(A::Mp3)),
    f("M4A · AAC", "m4a", Audio, None, Some(A::Aac)),
    f("Opus", "opus", Audio, None, Some(A::Opus)),
    f("OGG · Vorbis", "ogg", Audio, None, Some(A::Vorbis)),
    f("FLAC", "flac", Audio, None, Some(A::Flac)),
    f("WAV", "wav", Audio, None, Some(A::Wav)),
    f("M4A · ALAC", "m4a", Audio, None, Some(A::Alac)),
    f("AIFF", "aiff", Audio, None, Some(A::Aiff)),
    f("WMA", "wma", Audio, None, Some(A::Wma)),
    f("PNG", "png", Image, Some(V::Png), None),
    f("JPEG", "jpg", Image, Some(V::Jpeg), None),
    f("WebP", "webp", Image, Some(V::Webp), None),
    f("BMP", "bmp", Image, Some(V::Bmp), None),
    f("TIFF", "tiff", Image, Some(V::Tiff), None),
];

/// Linear map of quality (0..=100) from `worst` to `best`.
fn q_map(q: f32, worst: f32, best: f32) -> String {
    format!("{}", (worst + (best - worst) * (q / 100.0).clamp(0.0, 1.0)).round() as i32)
}

pub fn quality_hint(codec: Option<VCodec>, q: f32) -> String {
    match codec {
        Some(V::H264) => format!("CRF {}", q_map(q, 35.0, 14.0)),
        Some(V::H265) => format!("CRF {}", q_map(q, 38.0, 16.0)),
        Some(V::Vp9) => format!("CRF {}", q_map(q, 50.0, 18.0)),
        Some(V::Av1) => format!("CRF {}", q_map(q, 55.0, 20.0)),
        Some(V::ProRes) => ["Proxy", "Standard", "HQ"][((q / 34.0) as usize).min(2)].to_string(),
        Some(V::Mpeg4 | V::Mpeg2 | V::Wmv | V::Jpeg) => format!("q:v {}", q_map(q, 31.0, 2.0)),
        Some(V::Gif) => format!("{} fps", q_map(q, 8.0, 25.0)),
        Some(V::Webp | V::WebpAnim) => format!("quality {}", q_map(q, 0.0, 100.0)),
        Some(V::Png | V::Ffv1 | V::Bmp | V::Tiff | V::ProRes4444) => "lossless".into(),
        None => String::new(),
    }
}

/// Encoder arguments for a video/image codec (`-c:v ...` plus quality and pixel format).
pub fn video_args(codec: VCodec, q: f32) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mut a = match codec {
        V::H264 => s(&["-c:v", "libx264", "-preset", "medium", "-crf"]),
        V::H265 => s(&["-c:v", "libx265", "-preset", "medium", "-tag:v", "hvc1", "-crf"]),
        V::Vp9 => s(&["-c:v", "libvpx-vp9", "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "2", "-crf"]),
        V::Av1 => s(&["-c:v", "libsvtav1", "-preset", "8", "-crf"]),
        V::ProRes => s(&["-c:v", "prores_ks", "-pix_fmt", "yuv422p10le", "-profile:v"]),
        V::ProRes4444 => s(&["-c:v", "prores_ks", "-profile:v", "4", "-pix_fmt", "yuva444p10le"]),
        V::Mpeg4 => s(&["-c:v", "mpeg4", "-q:v"]),
        V::Mpeg2 => s(&["-c:v", "mpeg2video", "-q:v"]),
        V::Wmv => s(&["-c:v", "wmv2", "-q:v"]),
        V::Ffv1 => s(&["-c:v", "ffv1", "-level", "3"]),
        V::Gif => s(&["-c:v", "gif", "-loop", "0"]),
        V::WebpAnim => s(&["-c:v", "libwebp_anim", "-loop", "0", "-quality"]),
        V::Png => s(&["-c:v", "png"]),
        V::Jpeg => s(&["-c:v", "mjpeg", "-q:v"]),
        V::Webp => s(&["-c:v", "libwebp", "-quality"]),
        V::Bmp => s(&["-c:v", "bmp"]),
        V::Tiff => s(&["-c:v", "tiff", "-compression_algo", "lzw"]),
    };
    match codec {
        V::H264 => a.push(q_map(q, 35.0, 14.0)),
        V::H265 => a.push(q_map(q, 38.0, 16.0)),
        V::Vp9 => a.push(q_map(q, 50.0, 18.0)),
        V::Av1 => a.push(q_map(q, 55.0, 20.0)),
        V::ProRes => a.push(["0", "2", "3"][((q / 34.0) as usize).min(2)].into()),
        V::Mpeg4 | V::Mpeg2 | V::Wmv | V::Jpeg => a.push(q_map(q, 31.0, 2.0)),
        V::WebpAnim | V::Webp => a.push(q_map(q, 0.0, 100.0)),
        _ => {}
    }
    if matches!(codec, V::H264 | V::H265 | V::Vp9 | V::Av1 | V::Mpeg4 | V::Mpeg2 | V::Wmv) {
        a.extend(s(&["-pix_fmt", "yuv420p"]));
    }
    a
}

pub fn audio_args(codec: ACodec, q: f32, kbps: u32) -> Vec<String> {
    let b = format!("{kbps}k");
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match codec {
        A::Mp3 => s(&["-c:a", "libmp3lame", "-b:a", &b]),
        A::Aac => s(&["-c:a", "aac", "-b:a", &b]),
        A::Opus => s(&["-c:a", "libopus", "-b:a", &b]),
        A::Vorbis => s(&["-c:a", "libvorbis", "-q:a", &q_map(q, 0.0, 10.0)]),
        A::Flac => s(&["-c:a", "flac"]),
        A::Wav => s(&["-c:a", "pcm_s16le"]),
        A::Alac => s(&["-c:a", "alac"]),
        A::Aiff => s(&["-c:a", "pcm_s16be"]),
        A::Wma => s(&["-c:a", "wmav2", "-b:a", &b]),
        A::Mp2 => s(&["-c:a", "mp2", "-b:a", &b]),
    }
}

/// Filter appended for GIF output (palette generation gives far better colours).
pub fn gif_filter(q: f32) -> String {
    format!("fps={},{GIF_PALETTE}", q_map(q, 8.0, 25.0))
}

pub const GIF_PALETTE: &str = "split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=sierra2_4a";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Res {
    Keep,
    P2160,
    P1440,
    P1080,
    P720,
    P480,
    P360,
}

impl Res {
    pub const ALL: [Res; 7] = [Res::Keep, Res::P2160, Res::P1440, Res::P1080, Res::P720, Res::P480, Res::P360];
    pub fn label(self) -> &'static str {
        match self {
            Res::Keep => "Keep original",
            Res::P2160 => "2160p (4K)",
            Res::P1440 => "1440p",
            Res::P1080 => "1080p",
            Res::P720 => "720p",
            Res::P480 => "480p",
            Res::P360 => "360p",
        }
    }
    pub fn height(self) -> Option<u32> {
        match self {
            Res::Keep => None,
            Res::P2160 => Some(2160),
            Res::P1440 => Some(1440),
            Res::P1080 => Some(1080),
            Res::P720 => Some(720),
            Res::P480 => Some(480),
            Res::P360 => Some(360),
        }
    }
    /// Downscale filter (never upscales), if any is needed.
    pub fn filter(self, source_height: Option<u32>) -> Option<String> {
        let h = self.height()?;
        match source_height {
            Some(src) if src <= h => None,
            _ => Some(format!("scale=-2:{h}")),
        }
    }
}

pub const IMAGE_SEQ_FORMATS: &[(&str, &str, VCodec)] = &[
    ("PNG", "png", V::Png),
    ("JPEG", "jpg", V::Jpeg),
    ("WebP", "webp", V::Webp),
    ("BMP", "bmp", V::Bmp),
    ("TIFF", "tiff", V::Tiff),
];

/// Targets when building a video from an image sequence.
pub const SEQ_VIDEO_FORMATS: &[(&str, &str, VCodec)] = &[
    ("MP4 · H.264", "mp4", V::H264),
    ("WebM · VP9", "webm", V::Vp9),
    ("MOV · ProRes 4444 (alpha)", "mov", V::ProRes4444),
    ("MKV · FFV1 lossless", "mkv", V::Ffv1),
    ("GIF · animated", "gif", V::Gif),
    ("WebP · animated", "webp", V::WebpAnim),
    ("MP4 · H.265", "mp4", V::H265),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_maps_to_crf() {
        assert_eq!(video_args(V::H264, 100.0)[5], "14");
        assert_eq!(video_args(V::H264, 0.0)[5], "35");
        assert!(video_args(V::H264, 50.0).contains(&"yuv420p".to_string()));
    }

    #[test]
    fn res_never_upscales() {
        assert_eq!(Res::P720.filter(Some(480)), None);
        assert_eq!(Res::P720.filter(Some(1080)).as_deref(), Some("scale=-2:720"));
        assert_eq!(Res::Keep.filter(Some(1080)), None);
    }
}
