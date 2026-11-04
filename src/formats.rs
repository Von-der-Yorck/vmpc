//! Output format catalogue and the mapping from simple UI settings (quality 0-100,
//! resolution, bitrate) to FFmpeg encoder arguments. Formats whose encoder is missing
//! from the installed FFmpeg build are hidden at runtime ([`Format::available`]).

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
    Vp8,
    Vp9,
    Av1,
    Theora,
    ProRes,
    ProRes4444,
    Mpeg4,
    Mpeg2,
    Wmv,
    Ffv1,
    Mjpeg,
    Gif,
    WebpAnim,
    Apng,
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
    Tga,
    Ppm,
    Pgm,
    Pbm,
    Pam,
    Pcx,
    Sgi,
    SunRaster,
    Xbm,
    Xwd,
    Dpx,
    Exr,
    Hdr,
    Jpeg2000,
    Jxl,
    Qoi,
    Wbmp,
    Fits,
    Ico,
    Avif,
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
    Ac3,
    Eac3,
    Dts,
    AmrNb,
    AmrWb,
    WavPack,
    Tta,
    Speex,
    Gsm,
    PcmU8,
}

#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub name: &'static str,
    pub ext: &'static str,
    pub kind: Kind,
    pub video: Option<VCodec>,
    pub audio: Option<ACodec>,
    /// Explicit muxer when FFmpeg can't guess it from the extension.
    pub muxer: Option<&'static str>,
}

const fn f(name: &'static str, ext: &'static str, kind: Kind, video: Option<VCodec>, audio: Option<ACodec>) -> Format {
    Format { name, ext, kind, video, audio, muxer: None }
}

const fn fm(name: &'static str, ext: &'static str, kind: Kind, video: Option<VCodec>, audio: Option<ACodec>, muxer: &'static str) -> Format {
    Format { name, ext, kind, video, audio, muxer: Some(muxer) }
}

use ACodec as A;
use Kind::*;
use VCodec as V;

pub const FORMATS: &[Format] = &[
    // Video
    f("MP4 · H.264", "mp4", Video, Some(V::H264), Some(A::Aac)),
    f("MP4 · H.265", "mp4", Video, Some(V::H265), Some(A::Aac)),
    f("MP4 · AV1", "mp4", Video, Some(V::Av1), Some(A::Aac)),
    f("MKV · H.264", "mkv", Video, Some(V::H264), Some(A::Aac)),
    f("MKV · H.265", "mkv", Video, Some(V::H265), Some(A::Aac)),
    f("MKV · VP9", "mkv", Video, Some(V::Vp9), Some(A::Opus)),
    f("MKV · AV1", "mkv", Video, Some(V::Av1), Some(A::Opus)),
    f("WebM · VP9", "webm", Video, Some(V::Vp9), Some(A::Opus)),
    f("WebM · VP8", "webm", Video, Some(V::Vp8), Some(A::Vorbis)),
    f("WebM · AV1", "webm", Video, Some(V::Av1), Some(A::Opus)),
    f("MOV · H.264", "mov", Video, Some(V::H264), Some(A::Aac)),
    f("MOV · ProRes", "mov", Video, Some(V::ProRes), Some(A::Wav)),
    f("M4V · H.264", "m4v", Video, Some(V::H264), Some(A::Aac)),
    f("AVI · MPEG-4", "avi", Video, Some(V::Mpeg4), Some(A::Mp3)),
    f("AVI · MJPEG", "avi", Video, Some(V::Mjpeg), Some(A::Wav)),
    f("MPG · MPEG-2", "mpg", Video, Some(V::Mpeg2), Some(A::Mp2)),
    f("VOB · DVD", "vob", Video, Some(V::Mpeg2), Some(A::Ac3)),
    f("TS · H.264", "ts", Video, Some(V::H264), Some(A::Aac)),
    f("M2TS · H.264", "m2ts", Video, Some(V::H264), Some(A::Ac3)),
    f("MXF · MPEG-2", "mxf", Video, Some(V::Mpeg2), Some(A::Wav)),
    f("3GP · H.264", "3gp", Video, Some(V::H264), Some(A::Aac)),
    f("3G2 · H.264", "3g2", Video, Some(V::H264), Some(A::Aac)),
    f("FLV · H.264", "flv", Video, Some(V::H264), Some(A::Aac)),
    f("F4V · H.264", "f4v", Video, Some(V::H264), Some(A::Aac)),
    f("OGV · Theora", "ogv", Video, Some(V::Theora), Some(A::Vorbis)),
    f("WMV", "wmv", Video, Some(V::Wmv), Some(A::Wma)),
    f("ASF · WMV", "asf", Video, Some(V::Wmv), Some(A::Wma)),
    f("NUT · FFV1", "nut", Video, Some(V::Ffv1), Some(A::Flac)),
    f("MKV · FFV1 lossless", "mkv", Video, Some(V::Ffv1), Some(A::Flac)),
    f("GIF · animated", "gif", Video, Some(V::Gif), None),
    f("WebP · animated", "webp", Video, Some(V::WebpAnim), None),
    f("APNG · animated", "apng", Video, Some(V::Apng), None),
    // Audio
    f("MP3", "mp3", Audio, None, Some(A::Mp3)),
    f("M4A · AAC", "m4a", Audio, None, Some(A::Aac)),
    f("AAC", "aac", Audio, None, Some(A::Aac)),
    f("Opus", "opus", Audio, None, Some(A::Opus)),
    f("OGG · Vorbis", "ogg", Audio, None, Some(A::Vorbis)),
    f("FLAC", "flac", Audio, None, Some(A::Flac)),
    f("WAV", "wav", Audio, None, Some(A::Wav)),
    f("M4A · ALAC", "m4a", Audio, None, Some(A::Alac)),
    f("AIFF", "aiff", Audio, None, Some(A::Aiff)),
    f("WMA", "wma", Audio, None, Some(A::Wma)),
    f("AC3 · Dolby Digital", "ac3", Audio, None, Some(A::Ac3)),
    f("E-AC3 · Dolby Digital Plus", "eac3", Audio, None, Some(A::Eac3)),
    f("DTS", "dts", Audio, None, Some(A::Dts)),
    f("MP2", "mp2", Audio, None, Some(A::Mp2)),
    f("WavPack", "wv", Audio, None, Some(A::WavPack)),
    f("TTA · True Audio", "tta", Audio, None, Some(A::Tta)),
    f("CAF · ALAC", "caf", Audio, None, Some(A::Alac)),
    f("MKA · FLAC", "mka", Audio, None, Some(A::Flac)),
    fm("WebA · Opus", "weba", Audio, None, Some(A::Opus), "webm"),
    f("M4B · audiobook", "m4b", Audio, None, Some(A::Aac)),
    fm("M4R · iPhone ringtone", "m4r", Audio, None, Some(A::Aac), "ipod"),
    f("AU · Sun audio", "au", Audio, None, Some(A::Aiff)),
    f("W64 · Wave64", "w64", Audio, None, Some(A::Wav)),
    f("VOC · Creative", "voc", Audio, None, Some(A::PcmU8)),
    f("AMR-NB · voice", "amr", Audio, None, Some(A::AmrNb)),
    fm("AMR-WB · voice", "awb", Audio, None, Some(A::AmrWb), "amr"),
    f("Speex · voice", "spx", Audio, None, Some(A::Speex)),
    f("GSM · voice", "gsm", Audio, None, Some(A::Gsm)),
    // Image
    f("PNG", "png", Image, Some(V::Png), None),
    f("JPEG", "jpg", Image, Some(V::Jpeg), None),
    f("WebP", "webp", Image, Some(V::Webp), None),
    f("AVIF", "avif", Image, Some(V::Avif), None),
    f("JPEG XL", "jxl", Image, Some(V::Jxl), None),
    f("JPEG 2000", "jp2", Image, Some(V::Jpeg2000), None),
    f("BMP", "bmp", Image, Some(V::Bmp), None),
    f("TIFF", "tiff", Image, Some(V::Tiff), None),
    f("GIF", "gif", Image, Some(V::Gif), None),
    f("ICO · icon", "ico", Image, Some(V::Ico), None),
    f("TGA · Targa", "tga", Image, Some(V::Tga), None),
    f("QOI", "qoi", Image, Some(V::Qoi), None),
    f("OpenEXR · HDR", "exr", Image, Some(V::Exr), None),
    f("Radiance HDR", "hdr", Image, Some(V::Hdr), None),
    f("DPX · cinema", "dpx", Image, Some(V::Dpx), None),
    f("PPM", "ppm", Image, Some(V::Ppm), None),
    f("PGM · grayscale", "pgm", Image, Some(V::Pgm), None),
    f("PBM · black & white", "pbm", Image, Some(V::Pbm), None),
    f("PAM", "pam", Image, Some(V::Pam), None),
    f("PCX", "pcx", Image, Some(V::Pcx), None),
    f("SGI", "sgi", Image, Some(V::Sgi), None),
    f("Sun Raster", "ras", Image, Some(V::SunRaster), None),
    f("XBM", "xbm", Image, Some(V::Xbm), None),
    f("XWD", "xwd", Image, Some(V::Xwd), None),
    f("WBMP", "wbmp", Image, Some(V::Wbmp), None),
    f("FITS · astronomy", "fits", Image, Some(V::Fits), None),
];

/// FFmpeg encoder behind a video/image codec.
pub fn video_encoder(codec: VCodec) -> &'static str {
    match codec {
        V::H264 => "libx264",
        V::H265 => "libx265",
        V::Vp8 => "libvpx",
        V::Vp9 => "libvpx-vp9",
        V::Av1 => "libsvtav1",
        V::Theora => "libtheora",
        V::ProRes | V::ProRes4444 => "prores_ks",
        V::Mpeg4 => "mpeg4",
        V::Mpeg2 => "mpeg2video",
        V::Wmv => "wmv2",
        V::Ffv1 => "ffv1",
        V::Mjpeg | V::Jpeg => "mjpeg",
        V::Gif => "gif",
        V::WebpAnim => "libwebp_anim",
        V::Apng => "apng",
        V::Png | V::Ico => "png",
        V::Webp => "libwebp",
        V::Bmp => "bmp",
        V::Tiff => "tiff",
        V::Tga => "targa",
        V::Ppm => "ppm",
        V::Pgm => "pgm",
        V::Pbm => "pbm",
        V::Pam => "pam",
        V::Pcx => "pcx",
        V::Sgi => "sgi",
        V::SunRaster => "sunrast",
        V::Xbm => "xbm",
        V::Xwd => "xwd",
        V::Dpx => "dpx",
        V::Exr => "exr",
        V::Hdr => "hdr",
        V::Jpeg2000 => "jpeg2000",
        V::Jxl => "libjxl",
        V::Qoi => "qoi",
        V::Wbmp => "wbmp",
        V::Fits => "fits",
        V::Avif => "libaom-av1",
    }
}

/// FFmpeg encoder behind an audio codec.
pub fn audio_encoder(codec: ACodec) -> &'static str {
    match codec {
        A::Mp3 => "libmp3lame",
        A::Aac => "aac",
        A::Opus => "libopus",
        A::Vorbis => "libvorbis",
        A::Flac => "flac",
        A::Wav => "pcm_s16le",
        A::Alac => "alac",
        A::Aiff => "pcm_s16be",
        A::Wma => "wmav2",
        A::Mp2 => "mp2",
        A::Ac3 => "ac3",
        A::Eac3 => "eac3",
        A::Dts => "dca",
        A::AmrNb => "libopencore_amrnb",
        A::AmrWb => "libvo_amrwbenc",
        A::WavPack => "wavpack",
        A::Tta => "tta",
        A::Speex => "libspeex",
        A::Gsm => "libgsm",
        A::PcmU8 => "pcm_u8",
    }
}

impl Format {
    /// Whether the installed FFmpeg has the encoders this format needs.
    pub fn available(&self) -> bool {
        let video_ok = self.video.is_none_or(|v| crate::ffmpeg::has_encoder(video_encoder(v)));
        let audio_ok = self.audio.is_none_or(|a| crate::ffmpeg::has_encoder(audio_encoder(a)));
        video_ok && audio_ok
    }
}

/// Linear map of quality (0..=100) from `worst` to `best`.
fn q_map(q: f32, worst: f32, best: f32) -> String {
    format!("{}", (worst + (best - worst) * (q / 100.0).clamp(0.0, 1.0)).round() as i32)
}

pub fn quality_hint(codec: Option<VCodec>, q: f32) -> String {
    match codec {
        Some(V::H264) => format!("CRF {}", q_map(q, 35.0, 14.0)),
        Some(V::H265) => format!("CRF {}", q_map(q, 38.0, 16.0)),
        Some(V::Vp8) => format!("CRF {}", q_map(q, 50.0, 6.0)),
        Some(V::Vp9) => format!("CRF {}", q_map(q, 50.0, 18.0)),
        Some(V::Av1 | V::Avif) => format!("CRF {}", q_map(q, 55.0, 20.0)),
        Some(V::Theora) => format!("q {}", q_map(q, 1.0, 10.0)),
        Some(V::ProRes) => ["Proxy", "Standard", "HQ"][((q / 34.0) as usize).min(2)].to_string(),
        Some(V::Mpeg4 | V::Mpeg2 | V::Wmv | V::Jpeg | V::Mjpeg) => format!("q:v {}", q_map(q, 31.0, 2.0)),
        Some(V::Gif) => format!("{} fps", q_map(q, 8.0, 25.0)),
        Some(V::Webp | V::WebpAnim) => format!("quality {}", q_map(q, 0.0, 100.0)),
        Some(V::Jxl) => format!("distance {:.1}", jxl_distance(q)),
        Some(_) => "lossless".into(),
        None => String::new(),
    }
}

/// libjxl distance: 0 = lossless … 6 = small; quality 100 → 0.5, 0 → 6.
fn jxl_distance(q: f32) -> f32 {
    0.5 + 5.5 * (1.0 - (q / 100.0).clamp(0.0, 1.0))
}

/// Encoder arguments for a video/image codec (`-c:v ...` plus quality and pixel format).
pub fn video_args(codec: VCodec, q: f32) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let enc = video_encoder(codec);
    let mut a = s(&["-c:v", enc]);
    match codec {
        V::H264 => a.extend(s(&["-preset", "medium", "-crf", &q_map(q, 35.0, 14.0)])),
        V::H265 => a.extend(s(&["-preset", "medium", "-tag:v", "hvc1", "-crf", &q_map(q, 38.0, 16.0)])),
        V::Vp8 => a.extend(s(&["-b:v", "8M", "-crf", &q_map(q, 50.0, 6.0), "-deadline", "good", "-cpu-used", "2"])),
        V::Vp9 => a.extend(s(&["-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "2", "-crf", &q_map(q, 50.0, 18.0)])),
        V::Av1 => a.extend(s(&["-preset", "8", "-crf", &q_map(q, 55.0, 20.0)])),
        V::Avif => a.extend(s(&["-still-picture", "1", "-cpu-used", "6", "-crf", &q_map(q, 55.0, 20.0), "-pix_fmt", "yuv420p"])),
        V::Theora => a.extend(s(&["-q:v", &q_map(q, 1.0, 10.0)])),
        V::ProRes => a.extend(s(&["-pix_fmt", "yuv422p10le", "-profile:v", ["0", "2", "3"][((q / 34.0) as usize).min(2)]])),
        V::ProRes4444 => a.extend(s(&["-profile:v", "4", "-pix_fmt", "yuva444p10le"])),
        V::Mpeg4 | V::Mpeg2 | V::Wmv | V::Jpeg | V::Mjpeg => a.extend(s(&["-q:v", &q_map(q, 31.0, 2.0)])),
        V::Ffv1 => a.extend(s(&["-level", "3"])),
        V::Gif => a.extend(s(&["-loop", "0"])),
        V::WebpAnim => a.extend(s(&["-loop", "0", "-quality", &q_map(q, 0.0, 100.0)])),
        V::Apng => a.extend(s(&["-plays", "0"])),
        V::Webp => a.extend(s(&["-quality", &q_map(q, 0.0, 100.0)])),
        V::Tiff => a.extend(s(&["-compression_algo", "lzw"])),
        V::Jxl => a.extend(s(&["-distance", &format!("{:.2}", jxl_distance(q))])),
        // The ICO muxer only accepts PNG images with an alpha channel.
        V::Ico => a.extend(s(&["-pix_fmt", "rgba"])),
        _ => {}
    }
    if matches!(codec, V::H264 | V::H265 | V::Vp8 | V::Vp9 | V::Av1 | V::Theora | V::Mpeg4 | V::Mpeg2 | V::Wmv) {
        a.extend(s(&["-pix_fmt", "yuv420p"]));
    }
    if codec == V::Mjpeg {
        a.extend(s(&["-pix_fmt", "yuvj420p"]));
    }
    a
}

pub fn audio_args(codec: ACodec, q: f32, kbps: u32) -> Vec<String> {
    let b = format!("{kbps}k");
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let enc = audio_encoder(codec);
    let mut a = s(&["-c:a", enc]);
    match codec {
        A::Mp3 | A::Aac | A::Opus | A::Wma | A::Mp2 | A::Ac3 | A::Eac3 => a.extend(s(&["-b:a", &b])),
        A::Vorbis => a.extend(s(&["-q:a", &q_map(q, 0.0, 10.0)])),
        // DTS: the encoder is marked experimental and wants 44.1/48 kHz.
        A::Dts => a.extend(s(&["-strict", "-2", "-ar", "48000", "-b:a", &format!("{}k", kbps.max(320))])),
        // Voice codecs only take mono at their native rates.
        A::AmrNb => a.extend(s(&["-ar", "8000", "-ac", "1", "-b:a", "12.2k"])),
        A::AmrWb => a.extend(s(&["-ar", "16000", "-ac", "1", "-b:a", "23.85k"])),
        A::Speex => a.extend(s(&["-ar", "16000", "-ac", "1"])),
        A::Gsm => a.extend(s(&["-ar", "8000", "-ac", "1"])),
        _ => {}
    }
    a
}

/// Whether the codec takes a bitrate from the UI.
pub fn uses_bitrate(codec: ACodec) -> bool {
    matches!(codec, A::Mp3 | A::Aac | A::Opus | A::Wma | A::Mp2 | A::Ac3 | A::Eac3)
}

/// Subtitle outputs: (label, extension, explicit encoder). FFmpeg can read more
/// (SUB, SMI, SBV, …) than it can write, so only these are offered.
pub const SUB_FORMATS: &[(&str, &str, Option<&str>)] = &[
    ("SRT", "srt", None),
    ("WebVTT", "vtt", None),
    ("ASS", "ass", None),
    ("SSA", "ssa", Some("ssa")),
    ("TTML", "ttml", None),
    ("LRC · lyrics", "lrc", None),
];

/// Picture-based subtitle codecs (DVD/Blu-ray) that can't become text subtitles.
pub fn is_bitmap_subtitle(codec: &str) -> bool {
    matches!(codec, "hdmv_pgs_subtitle" | "dvd_subtitle" | "dvb_subtitle" | "xsub" | "dvb_teletext")
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
    ("TGA", "tga", V::Tga),
    ("DPX", "dpx", V::Dpx),
    ("OpenEXR", "exr", V::Exr),
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
    ("APNG · animated", "apng", V::Apng),
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
