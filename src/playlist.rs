//! Playlist files: M3U/M3U8, PLS, XSPF, ASX/WAX/WVX and WPL/ZPL. Opening one loads its
//! entries into the library. HLS media playlists (.m3u8 with `#EXT-X-` tags) are
//! streams, not lists, and are handed to FFmpeg instead.

use std::path::Path;

/// HLS stream (segments / variants) rather than a list of files.
pub fn is_hls(text: &str) -> bool {
    text.contains("#EXT-X-TARGETDURATION") || text.contains("#EXT-X-STREAM-INF") || text.contains("#EXT-X-MEDIA-SEQUENCE")
}

/// Entries of a playlist, resolved against the playlist's folder.
pub fn parse(text: &str, ext: &str, base: Option<&Path>) -> Vec<String> {
    let raw: Vec<String> = match ext {
        "pls" => text
            .lines()
            .filter_map(|l| {
                let (key, value) = l.split_once('=')?;
                key.trim().to_ascii_lowercase().starts_with("file").then(|| value.trim().to_string())
            })
            .collect(),
        "xspf" => xml_values(text, "location"),
        "asx" | "wax" | "wvx" => xml_attr_values(text, "ref", "href"),
        "wpl" | "zpl" => xml_attr_values(text, "media", "src"),
        // m3u / m3u8 and anything list-like: one entry per non-comment line.
        _ => text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect(),
    };
    raw.into_iter().filter(|e| !e.is_empty()).map(|e| resolve(&unescape(&e), base)).collect()
}

fn resolve(entry: &str, base: Option<&Path>) -> String {
    if let Some(rest) = entry.strip_prefix("file:///") {
        // file:///C:/a%20b.mp3 or file:///home/x.mp3
        let path = percent_decode(rest);
        return if cfg!(windows) || path.chars().nth(1) == Some(':') { path } else { format!("/{path}") };
    }
    if entry.contains("://") {
        return entry.to_string();
    }
    let path = Path::new(entry);
    if path.is_absolute() {
        return entry.to_string();
    }
    match base {
        Some(b) => b.join(entry.replace('\\', std::path::MAIN_SEPARATOR_STR)).to_string_lossy().into_owned(),
        None => entry.to_string(),
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'")
}

/// Text content of every `<tag>…</tag>` (case-insensitive tag names).
fn xml_values(text: &str, tag: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find(&open) {
        let from = pos + start + open.len();
        let Some(end) = lower[from..].find(&close) else { break };
        out.push(text[from..from + end].trim().to_string());
        pos = from + end + close.len();
    }
    out
}

/// Value of `attr` on every `<tag …>` element (case-insensitive).
fn xml_attr_values(text: &str, tag: &str, attr: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let open = format!("<{tag}");
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find(&open) {
        let from = pos + start;
        let end = lower[from..].find('>').map(|e| from + e).unwrap_or(lower.len());
        let element = &lower[from..end];
        if let Some(a) = element.find(&format!("{attr}=")) {
            let value_start = from + a + attr.len() + 1;
            let quote = text[value_start..].chars().next().unwrap_or('"');
            if quote == '"' || quote == '\'' {
                if let Some(close) = text[value_start + 1..].find(quote) {
                    out.push(text[value_start + 1..value_start + 1 + close].to_string());
                }
            }
        }
        pos = end.max(from + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> std::path::PathBuf {
        std::path::PathBuf::from(if cfg!(windows) { "C:\\music" } else { "/music" })
    }

    #[test]
    fn m3u_and_pls() {
        let m3u = "#EXTM3U\n#EXTINF:123,Artist - Song\nsong.mp3\n\nhttp://radio.example/stream\n";
        let entries = parse(m3u, "m3u", Some(&base()));
        assert_eq!(entries.len(), 2);
        assert!(entries[0].ends_with("song.mp3") && entries[0].contains("music"));
        assert_eq!(entries[1], "http://radio.example/stream");
        let pls = "[playlist]\nFile1=a.flac\nTitle1=A\nFile2=http://x/y\nNumberOfEntries=2\n";
        assert_eq!(parse(pls, "pls", None), vec!["a.flac".to_string(), "http://x/y".to_string()]);
    }

    #[test]
    fn xml_playlists() {
        let xspf = "<playlist><trackList><track><location>file:///C:/Music/a%20b.mp3</location></track>\
                    <track><location>https://x/c.ogg</location></track></trackList></playlist>";
        let e = parse(xspf, "xspf", None);
        assert_eq!(e[1], "https://x/c.ogg");
        assert!(e[0].ends_with("a b.mp3"));
        let asx = r#"<asx version="3.0"><entry><ref href="http://s/1.wma"/></entry><Entry><REF HREF='2.wmv'/></Entry></asx>"#;
        assert_eq!(parse(asx, "asx", None), vec!["http://s/1.wma".to_string(), "2.wmv".to_string()]);
        let wpl = r#"<smil><body><seq><media src="..\Music\x.mp3"/><media src="y &amp; z.mp3"/></seq></body></smil>"#;
        let w = parse(wpl, "wpl", None);
        assert_eq!(w.len(), 2);
        assert_eq!(w[1], "y & z.mp3");
    }

    #[test]
    fn hls_is_a_stream() {
        assert!(is_hls("#EXTM3U\n#EXT-X-TARGETDURATION:10\nseg1.ts\n"));
        assert!(!is_hls("#EXTM3U\nsong.mp3\n"));
    }
}
