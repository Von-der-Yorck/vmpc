//! Small helpers shared across the app: time formatting/parsing, natural sort,
//! collision-free output paths.

use std::path::{Path, PathBuf};

/// Formats seconds as `h:mm:ss` or `m:ss`.
pub fn fmt_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "--:--".into();
    }
    let total = secs.floor() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Parses `ss`, `ss.ms`, `mm:ss` or `hh:mm:ss(.ms)`. Empty input means "not set".
pub fn parse_time(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for part in text.split(':') {
        let v: f64 = part.trim().replace(',', ".").parse().ok()?;
        total = total * 60.0 + v;
    }
    (total >= 0.0).then_some(total)
}

/// Compares strings so that `img2` sorts before `img10`.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(ca), Some(cb)) => {
                let ord = ca.to_ascii_lowercase().cmp(&cb.to_ascii_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Returns `dir/stem.ext`, or `dir/stem (2).ext` etc. if that already exists.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    candidate
}

/// Same as [`unique_path`] but for directories.
pub fn unique_dir(parent: &Path, name: &str) -> PathBuf {
    let mut candidate = parent.join(name);
    let mut n = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{name} ({n})"));
        n += 1;
    }
    candidate
}

/// File name for display (falls back to the full string for URLs).
pub fn display_name(source: &str) -> String {
    if is_url(source) {
        return source.to_string();
    }
    Path::new(source)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string())
}

/// Shows a file (selected) or folder in the system file manager.
pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("explorer");
        if path.is_file() {
            let mut arg = std::ffi::OsString::from("/select,");
            arg.push(path);
            cmd.arg(arg);
        } else {
            cmd.arg(path);
        }
        let _ = cmd.spawn();
    }
    #[cfg(not(windows))]
    {
        let target = if path.is_file() { path.parent().unwrap_or(path) } else { path };
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

/// Default folder for outputs that have no obvious home (URLs, snapshots).
pub fn default_output_dir(kind: &str) -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = home.join(kind);
    if dir.is_dir() {
        dir
    } else {
        home
    }
}

pub fn is_url(source: &str) -> bool {
    source.contains("://")
}

pub const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp", "gif", "tga", "exr", "dpx", "ppm", "pgm",
    "jxl", "avif", "heic", "qoi",
];

pub fn is_image_path(p: &Path) -> bool {
    p.extension()
        .map(|e| IMAGE_EXTS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_roundtrip() {
        assert_eq!(fmt_time(3725.4), "1:02:05");
        assert_eq!(fmt_time(65.0), "1:05");
        assert_eq!(parse_time("1:02:05"), Some(3725.0));
        assert_eq!(parse_time("90,5"), Some(90.5));
        assert_eq!(parse_time(""), None);
        assert_eq!(parse_time("abc"), None);
    }

    #[test]
    fn natural_sorting() {
        let mut v = vec!["f10.png", "f2.png", "F1.png", "f002b.png"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["F1.png", "f2.png", "f002b.png", "f10.png"]);
    }
}
