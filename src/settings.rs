//! User preferences, stored as JSON in the platform config folder
//! (`%APPDATA%\VMPC\settings.json` or `~/.config/vmpc/settings.json`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    pub volume: f32,
    pub muted: bool,
    /// Maximum decode height for playback (720, 1080, 1440, 2160).
    pub max_height: u32,
    pub hwaccel: bool,
    /// 0 = off, 1 = playlist, 2 = current item.
    pub loop_mode: u8,
    /// Whether the tools inspector next to the preview is open.
    pub tools_open: bool,
    /// Check GitHub for a newer release at every start.
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { dark: false, volume: 0.8, muted: false, max_height: 1080, hwaccel: true, loop_mode: 0, tools_open: true, auto_update: true }
    }
}

fn path() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("VMPC"));
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join("vmpc"));
    base.map(|d| d.join("settings.json"))
}

pub fn load() -> Settings {
    path()
        .and_then(|p| std::fs::read(p).ok())
        // Tolerate a UTF-8 byte-order mark (added by Notepad or PowerShell when hand-edited).
        .and_then(|bytes| serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"dark": true, "volume": 0.5}"#).unwrap();
        assert!(s.dark);
        assert_eq!(s.volume, 0.5);
        assert_eq!(s.max_height, 1080);
        assert!(s.hwaccel);
    }

    #[test]
    fn old_files_and_bom_still_load() {
        // v1.0 files had "show_playlist", which is simply ignored now.
        let bytes = b"\xEF\xBB\xBF{\"dark\": true, \"show_playlist\": true}";
        let s: Settings = serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap()).unwrap();
        assert!(s.dark);
        assert!(s.tools_open);
    }
}
