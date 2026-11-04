//! What VMPC knows about file formats: ~1,700 extensions from Wikipedia's
//! "List of file formats" (assets/formats.tsv, CC BY-SA 4.0) and how each kind of
//! file is handled: played/converted with FFmpeg, opened as a playlist, browsed as an
//! archive, viewed (and for data files converted) as text, or recognised by name.

use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

use crate::ffmpeg::{self, MediaInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub ext: &'static str,
    pub category: &'static str,
    pub section: &'static str,
    pub desc: &'static str,
}

static DATA: &str = include_str!("../assets/formats.tsv");
static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();

/// Every catalogued format (an extension can appear in several categories).
pub fn all() -> &'static [Entry] {
    ENTRIES.get_or_init(|| {
        DATA.lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let mut c = l.split('\t');
                Some(Entry { ext: c.next()?, category: c.next()?, section: c.next()?, desc: c.next().unwrap_or("") })
            })
            .collect()
    })
}

pub fn lookup(ext: &str) -> Vec<&'static Entry> {
    let ext = ext.to_ascii_lowercase();
    all().iter().filter(|e| e.ext == ext).collect()
}

/// Lower-case extension, folding compound archive suffixes (`.tar.gz` → `tgz`).
pub fn ext_of(source: &str) -> String {
    let name = Path::new(source).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    for (compound, short) in [(".tar.gz", "tgz"), (".tar.bz2", "tbz2"), (".tar.xz", "txz"), (".tar.zst", "tzst"), (".tar.lz", "tlz")] {
        if name.ends_with(compound) {
            return short.into();
        }
    }
    Path::new(&name).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataFormat {
    Json,
    Yaml,
    Toml,
    Csv,
    Tsv,
}

impl DataFormat {
    pub const ALL: [DataFormat; 5] = [DataFormat::Json, DataFormat::Yaml, DataFormat::Toml, DataFormat::Csv, DataFormat::Tsv];
    pub fn from_ext(ext: &str) -> Option<Self> {
        Some(match ext {
            "json" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "csv" => Self::Csv,
            "tsv" | "tab" => Self::Tsv,
            _ => return None,
        })
    }
    pub fn ext(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
        }
    }
}

/// How VMPC treats a format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handler {
    /// Played / shown and converted with FFmpeg.
    Media,
    Subtitle,
    Playlist,
    Archive,
    /// Text data that converts between JSON / YAML / TOML / CSV / TSV.
    Data(DataFormat),
    /// Shown in the text viewer.
    Text,
    /// Known format VMPC can't open itself (documents, CAD, executables …).
    Recognized,
    Unknown,
}

impl Handler {
    pub fn label(self) -> &'static str {
        match self {
            Handler::Media => "Play · convert",
            Handler::Subtitle => "View · convert subtitles",
            Handler::Playlist => "Open playlist",
            Handler::Archive => "Browse · extract · repack",
            Handler::Data(_) => "View · convert data",
            Handler::Text => "View as text",
            Handler::Recognized => "Recognized",
            Handler::Unknown => "Unknown",
        }
    }
}

pub const SUBTITLE_EXTS: &[&str] =
    &["srt", "ass", "ssa", "vtt", "webvtt", "sub", "lrc", "sbv", "ttml", "dfxp", "smi", "sami", "mpl", "rt", "jss", "pjs", "scc"];

pub const PLAYLIST_EXTS: &[&str] = &["m3u", "m3u8", "pls", "xspf", "asx", "wpl", "zpl", "wax", "wvx"];

pub const ARCHIVE_EXTS: &[&str] = &[
    "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz", "tbz2", "xz", "txz", "zst", "tzst", "lz", "tlz", "lzma", "lz4", "z",
    "cab", "iso", "cpio", "xar", "lzh", "lha", "ar", "deb", "rpm", "cbz", "cbr", "cb7", "cbt", "jar", "war", "apk", "warc",
];

/// Extensions FFmpeg demuxes/decodes (the probe has the final word).
pub const MEDIA_EXTS: &[&str] = &[
    // video
    "3gp", "3g2", "asf", "avi", "bik", "bk2", "dv", "dvr-ms", "f4v", "flv", "h264", "264", "hevc", "h265", "ivf", "m1v", "m2v",
    "m2ts", "mts", "mk3d", "mkv", "mov", "mp4", "m4v", "mpeg", "mpg", "mpe", "mxf", "nsv", "nut", "ogv", "rm", "rmvb", "roq", "smk",
    "thp", "ts", "vob", "webm", "wmv", "wtv", "y4m", "amv", "mjpeg", "divx", "str", "qt", "swf", "avs", "gxf", "mve", "vp6",
    // audio
    "8svx", "16svx", "aiff", "aif", "aifc", "au", "snd", "bwf", "wav", "w64", "rf64", "caf", "flac", "ape", "ofr", "ofs", "tak",
    "tta", "wv", "wma", "ra", "dts", "dtshd", "ac3", "eac3", "amr", "awb", "mp1", "mp2", "mp3", "spx", "gsm", "aac", "mpc",
    "ogg", "oga", "opus", "m4a", "m4b", "m4r", "mka", "weba", "mlp", "truehd", "thd", "shn", "voc", "dsf", "dff", "vqf", "xa",
    "mod", "s3m", "xm", "it", "mptm", "669", "stm", "umx", "mtm", "ult", "far", "med", "psf", "brstm", "bcwav", "ast", "smp",
    // images
    "bmp", "dib", "png", "apng", "jpg", "jpeg", "jfif", "jpe", "gif", "tif", "tiff", "webp", "tga", "pcx", "ppm", "pgm", "pbm",
    "pam", "pnm", "pfm", "sgi", "rgb", "rgba", "bw", "ras", "sun", "xbm", "xpm", "xwd", "dpx", "cin", "exr", "hdr", "jp2",
    "j2k", "jpx", "jxl", "avif", "heic", "heif", "psd", "dds", "qoi", "ico", "cur", "wbmp", "fits", "fit", "pix", "pcd", "jls",
    "vbn", "pictor", "sgx", "msp",
];

/// Extensions shown in the text viewer (besides anything that sniffs as text).
const TEXT_EXTS: &[&str] = &[
    "txt", "text", "md", "markdown", "rst", "log", "ini", "cfg", "conf", "config", "properties", "env", "xml", "html", "htm",
    "xhtml", "css", "js", "mjs", "cjs", "jsx", "tsx", "py", "pyw", "rs", "c", "h", "cpp", "cc", "cxx", "hpp", "hh", "cs",
    "java", "kt", "kts", "go", "rb", "php", "pl", "pm", "lua", "r", "swift", "scala", "sh", "bash", "zsh", "fish", "bat", "cmd",
    "ps1", "psm1", "vb", "vbs", "sql", "tex", "bib", "srt", "nfo", "diz", "gitignore", "gitattributes", "editorconfig", "dockerfile",
    "makefile", "cmake", "gradle", "svg", "rss", "atom", "plist", "reg", "inf", "asm", "s", "f90", "f", "for", "pas", "dart",
    "elm", "ex", "exs", "erl", "hs", "ml", "clj", "lisp", "el", "vim", "patch", "diff", "csv", "tsv", "json", "yaml", "yml",
    "toml", "ipynb", "geojson", "gpx", "kml", "vtt", "ass", "ssa", "lrc", "m3u", "pls",
];

/// Catalogue categories whose files are usually plain text.
const TEXT_CATEGORIES: &[&str] = &["Source code", "Web page"];

/// Handler decided from the extension alone (used for labels in the format library).
pub fn handler_for_ext(ext: &str) -> Handler {
    let ext = ext.to_ascii_lowercase();
    let e = ext.as_str();
    if PLAYLIST_EXTS.contains(&e) {
        Handler::Playlist
    } else if SUBTITLE_EXTS.contains(&e) {
        Handler::Subtitle
    } else if ARCHIVE_EXTS.contains(&e) {
        Handler::Archive
    } else if let Some(d) = DataFormat::from_ext(e) {
        Handler::Data(d)
    } else if MEDIA_EXTS.contains(&e) {
        Handler::Media
    } else if TEXT_EXTS.contains(&e) || lookup(e).iter().any(|x| TEXT_CATEGORIES.contains(&x.category) || x.section == "Text-based") {
        Handler::Text
    } else if !lookup(e).is_empty() {
        Handler::Recognized
    } else {
        Handler::Unknown
    }
}

/// Every catalogue entry with what VMPC does with it (for the format library).
pub fn with_handlers() -> &'static [(Entry, Handler)] {
    static ROWS: OnceLock<Vec<(Entry, Handler)>> = OnceLock::new();
    ROWS.get_or_init(|| all().iter().map(|e| (*e, handler_for_ext(e.ext))).collect())
}

/// Number of distinct extensions in the catalogue.
pub fn unique_extensions() -> usize {
    static COUNT: OnceLock<usize> = OnceLock::new();
    *COUNT.get_or_init(|| all().iter().map(|e| e.ext).collect::<std::collections::HashSet<_>>().len())
}

/// The result of looking inside a file.
#[derive(Clone, Debug)]
pub enum Classified {
    Media(MediaInfo),
    Subtitle(String),
    /// Entries (absolute paths or URLs).
    Playlist(Vec<String>),
    /// Entry names.
    Archive(Vec<String>),
    Data(DataFormat, String),
    Text(String),
    Recognized,
    Unreadable(String),
}

/// Text files are shown up to this size.
const TEXT_LIMIT: usize = 2 * 1024 * 1024;

/// Reads the start of a file as text, or `None` if it looks binary.
pub fn read_text(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.by_ref().take(TEXT_LIMIT as u64).read_to_end(&mut buf).ok()?;
    let probe = &buf[..buf.len().min(64 * 1024)];
    if probe.contains(&0) {
        return None;
    }
    let printable = probe.iter().filter(|b| b.is_ascii_graphic() || b.is_ascii_whitespace() || **b >= 0xA0).count();
    let mostly_printable = printable * 100 >= probe.len() * 95;
    let text = match String::from_utf8(buf) {
        Ok(s) => s,
        // Latin-1 / Windows-1252 text is still text: map bytes to chars.
        Err(_) if !mostly_printable => return None,
        Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
    };
    Some(text.trim_start_matches('\u{feff}').to_string())
}

/// Looks inside `source` and decides how to show it. Runs on a background thread.
pub fn classify(source: &str) -> Classified {
    if crate::util::is_url(source) {
        return match ffmpeg::probe(source) {
            Ok(info) => Classified::Media(info),
            Err(e) => Classified::Unreadable(e),
        };
    }
    let path = Path::new(source);
    if path.is_dir() {
        return Classified::Unreadable("This is a folder; open it with “Open folder”.".into());
    }
    let ext = ext_of(source);
    let probe_media = || ffmpeg::probe(source).ok().filter(|i| !matches!(i.format_name.as_str(), "tty" | "Tele-typewriter"));
    match handler_for_ext(&ext) {
        Handler::Playlist => {
            let text = read_text(path).unwrap_or_default();
            if crate::playlist::is_hls(&text) {
                return probe_media().map(Classified::Media).unwrap_or(Classified::Unreadable("Unreadable HLS stream".into()));
            }
            Classified::Playlist(crate::playlist::parse(&text, &ext, path.parent()))
        }
        Handler::Subtitle => match read_text(path) {
            Some(text) => Classified::Subtitle(text),
            // e.g. VobSub .sub (binary) or PGS: let FFmpeg try it as media.
            None => probe_media().map(Classified::Media).unwrap_or(Classified::Recognized),
        },
        Handler::Archive => match crate::archive::list(path) {
            Ok(entries) => Classified::Archive(entries),
            Err(e) => Classified::Unreadable(e),
        },
        Handler::Data(fmt) => match read_text(path) {
            Some(text) => Classified::Data(fmt, text),
            None => Classified::Unreadable("The file is not valid text.".into()),
        },
        Handler::Text => match read_text(path) {
            Some(text) => Classified::Text(text),
            None => Classified::Recognized,
        },
        Handler::Media | Handler::Unknown => match probe_media() {
            Some(info) => Classified::Media(info),
            None => match read_text(path) {
                Some(text) => Classified::Text(text),
                None if handler_for_ext(&ext) == Handler::Media => {
                    Classified::Unreadable(ffmpeg::probe(source).err().unwrap_or_else(|| "Unreadable media".into()))
                }
                None => Classified::Recognized,
            },
        },
        Handler::Recognized => Classified::Recognized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_and_finds_formats() {
        assert!(all().len() > 1800, "catalog has {} entries", all().len());
        assert!(lookup("PDF").iter().any(|e| e.desc.contains("Portable Document Format")));
        assert!(lookup("mp4").iter().any(|e| e.category == "Video"));
        assert_eq!(ext_of("C:/x/backup.TAR.GZ"), "tgz");
        assert_eq!(ext_of("/a/b/clip.MKV"), "mkv");
    }

    #[test]
    fn handlers_by_extension() {
        assert_eq!(handler_for_ext("mkv"), Handler::Media);
        assert_eq!(handler_for_ext("srt"), Handler::Subtitle);
        assert_eq!(handler_for_ext("pls"), Handler::Playlist);
        assert_eq!(handler_for_ext("7z"), Handler::Archive);
        assert_eq!(handler_for_ext("yml"), Handler::Data(DataFormat::Yaml));
        assert_eq!(handler_for_ext("rs"), Handler::Text);
        assert_eq!(handler_for_ext("dwg"), Handler::Recognized);
        assert_eq!(handler_for_ext("zzzz"), Handler::Unknown);
    }

    #[test]
    fn text_sniffing() {
        let dir = std::env::temp_dir().join(format!("vmpc_sniff_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("notes.unknownext");
        std::fs::write(&text, "hello\nworld\n").unwrap();
        let bin = dir.join("blob.unknownext");
        std::fs::write(&bin, [0u8, 159, 146, 150, 0, 1, 2]).unwrap();
        assert_eq!(read_text(&text).as_deref(), Some("hello\nworld\n"));
        assert!(read_text(&bin).is_none());
        assert!(matches!(classify(&text.to_string_lossy()), Classified::Text(_)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
