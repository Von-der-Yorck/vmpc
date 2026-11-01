//! Auto updater. At start-up VMPC asks GitHub for the latest release; when it is newer
//! than the running version the app offers to update. Updating downloads the release
//! asset for this platform, verifies it against the release's SHA256SUMS.txt and
//! installs it:
//!
//! * Windows (installed with the setup): runs the new setup silently over the existing
//!   install (same mode: per-user or all users); the setup restarts VMPC afterwards.
//! * Linux (installed with install-linux.sh into ~/.local/share/vmpc): unpacks the new
//!   tarball, runs its installer and restarts.
//! * Anything else (portable zip, .deb): opens the release page instead.
//!
//! Network problems never produce a prompt: an automatic check that cannot reach
//! GitHub simply does nothing.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::ffmpeg::{command, system_tool};

/// `owner/repo` the updates come from; release builds get it from the CI environment.
pub const REPO: &str = match option_env!("VMPC_REPO") {
    Some(repo) => repo,
    None => "Von-der-Yorck/vmpc",
};

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// The download for this platform, if the release has one.
    pub asset: Option<Asset>,
    pub sums_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading { release: Release, progress: f32 },
    Installing,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    WindowsSetup { all_users: bool },
    LinuxUser,
    Manual,
}

/// The running version. `VMPC_PRETEND_VERSION` overrides it to exercise the update path.
pub fn current_version() -> String {
    std::env::var("VMPC_PRETEND_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// `v1.2.3`, `1.2.3`, `1.2` or `1.2.3-beta` → (1, 2, 3).
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// How this copy of VMPC was installed (decides how it can update itself).
pub fn install_kind() -> InstallKind {
    let Some(exe) = std::env::current_exe().ok() else { return InstallKind::Manual };
    let Some(dir) = exe.parent() else { return InstallKind::Manual };
    if cfg!(windows) && dir.join("unins000.exe").is_file() {
        let lower = dir.to_string_lossy().to_lowercase();
        let all_users = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .any(|pf| !pf.is_empty() && lower.starts_with(&pf.to_lowercase()));
        return InstallKind::WindowsSetup { all_users };
    }
    if cfg!(not(windows)) {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")));
        if data.is_some_and(|d| dir == d.join("vmpc")) {
            return InstallKind::LinuxUser;
        }
    }
    InstallKind::Manual
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn platform_asset(name: &str) -> bool {
    if cfg!(windows) {
        name.starts_with("VMPC-Setup-") && name.ends_with("-x64.exe")
    } else {
        name == "vmpc-linux-x86_64.tar.gz"
    }
}

/// Parses the GitHub "latest release" JSON.
fn parse_release(json: &[u8]) -> Result<Release, String> {
    let r: ApiRelease = serde_json::from_slice(json).map_err(|e| format!("Unexpected answer from GitHub: {e}"))?;
    if r.draft || r.prerelease {
        return Err("No stable release".into());
    }
    let asset = r
        .assets
        .iter()
        .find(|a| platform_asset(&a.name))
        .map(|a| Asset { name: a.name.clone(), url: a.browser_download_url.clone(), size: a.size });
    let sums_url = r.assets.iter().find(|a| a.name == "SHA256SUMS.txt").map(|a| a.browser_download_url.clone());
    Ok(Release {
        version: r.tag_name.trim_start_matches(['v', 'V']).to_string(),
        notes: r.body.unwrap_or_default(),
        page: r.html_url,
        asset,
        sums_url,
    })
}

/// Runs `curl` and returns stdout (short timeouts: an offline PC must not hang the check).
fn fetch(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    let out = command(system_tool("curl"))
        .args(["-fsSL", "--connect-timeout", "5", "--max-time", "20", "-H"])
        .arg(format!("Accept: {accept}"))
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Finds `name`'s hash in a `sha256sum` style listing.
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name).then(|| hash.to_lowercase())
    })
}

/// The "what's new" part of release notes as plain lines (heading?, text): everything
/// before the "## Download" section, with Markdown markup removed.
pub fn whats_new(notes: &str) -> Vec<(bool, String)> {
    notes
        .lines()
        .take_while(|l| !l.trim_start().to_lowercase().starts_with("## download"))
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let t = l.trim();
            let heading = t.starts_with('#');
            let mut s = t.trim_start_matches('#').trim().replace("**", "").replace('`', "");
            if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
                s = format!("•  {rest}");
            }
            (heading, s)
        })
        .take(24)
        .collect()
}

/// Opens a URL in the default browser.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[derive(Clone)]
pub struct Updater {
    state: Arc<Mutex<State>>,
}

impl Default for Updater {
    fn default() -> Self {
        Self { state: Arc::new(Mutex::new(State::Idle)) }
    }
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, state: State, ctx: &egui::Context) {
        *self.state.lock().unwrap() = state;
        ctx.request_repaint();
    }

    /// Looks for a newer release in the background. With `manual == false` every
    /// failure (offline, GitHub unreachable, rate limit) is silent.
    pub fn check(&self, ctx: &egui::Context, manual: bool) {
        if matches!(self.state(), State::Checking | State::Downloading { .. } | State::Installing) {
            return;
        }
        if manual {
            self.set(State::Checking, ctx);
        }
        let (this, ctx) = (self.clone(), ctx.clone());
        std::thread::spawn(move || {
            let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
            let result = fetch(&url, "application/vnd.github+json").and_then(|json| parse_release(&json));
            let next = match result {
                Ok(release) if is_newer(&release.version, &current_version()) => State::Available(release),
                Ok(_) => State::UpToDate,
                Err(_) if !manual => State::Idle,
                Err(_) => State::Failed("GitHub couldn't be reached. VMPC will check again next time.".into()),
            };
            this.set(next, &ctx);
        });
    }

    /// Downloads, verifies and installs `release`; VMPC closes and restarts on success.
    pub fn install(&self, ctx: &egui::Context, release: Release) {
        let kind = install_kind();
        if kind == InstallKind::Manual || release.asset.is_none() {
            open_url(&release.page);
            self.set(State::Idle, ctx);
            return;
        }
        let (this, ctx) = (self.clone(), ctx.clone());
        this.set(State::Downloading { release: release.clone(), progress: 0.0 }, &ctx);
        std::thread::spawn(move || {
            if let Err(e) = this.download_and_install(&ctx, &release, kind) {
                this.set(State::Failed(e), &ctx);
            }
        });
    }

    fn download_and_install(&self, ctx: &egui::Context, release: &Release, kind: InstallKind) -> Result<(), String> {
        let asset = release.asset.as_ref().ok_or("no download for this platform")?;
        let dir = std::env::temp_dir().join("vmpc-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(&asset.name);

        // Download with live progress (file size vs. the size GitHub reported).
        let mut child = command(system_tool("curl"))
            .args(["-fsSL", "--connect-timeout", "10", "-o"])
            .arg(&file)
            .arg(&asset.url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("curl: {e}"))?;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            let done = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            let progress = (done as f32 / asset.size.max(1) as f32).min(1.0);
            self.set(State::Downloading { release: release.clone(), progress }, ctx);
            std::thread::sleep(Duration::from_millis(200));
        };
        if !status.success() {
            return Err("The download was interrupted. Please try again later.".into());
        }

        // Verify against the release's checksum list.
        let sums_url = release.sums_url.as_ref().ok_or("The release has no checksums; not installing it.")?;
        let sums = String::from_utf8_lossy(&fetch(sums_url, "text/plain")?).into_owned();
        let expected = expected_hash(&sums, &asset.name).ok_or("The download is not listed in the checksums.")?;
        if sha256_file(&file)? != expected {
            let _ = std::fs::remove_file(&file);
            return Err("The download is damaged (checksum mismatch); nothing was installed.".into());
        }

        self.set(State::Installing, ctx);
        match kind {
            InstallKind::WindowsSetup { all_users } => {
                // Inno Setup: silent install over the existing one, then relaunch VMPC.
                std::process::Command::new(&file)
                    .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/RELAUNCH=1"])
                    .arg(if all_users { "/ALLUSERS" } else { "/CURRENTUSER" })
                    .spawn()
                    .map_err(|e| format!("Couldn't start the installer: {e}"))?;
            }
            InstallKind::LinuxUser => {
                let ok = command(system_tool("tar")).arg("-xzf").arg(&file).arg("-C").arg(&dir).status().map_err(|e| e.to_string())?;
                if !ok.success() {
                    return Err("Couldn't unpack the update.".into());
                }
                let ok = command("sh").arg(dir.join("vmpc").join("install-linux.sh")).status().map_err(|e| e.to_string())?;
                if !ok.success() {
                    return Err("The update installer failed.".into());
                }
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                std::process::Command::new(exe).spawn().map_err(|e| e.to_string())?;
            }
            InstallKind::Manual => unreachable!("handled before downloading"),
        }
        // Close this window so the installer can replace our files.
        std::thread::sleep(Duration::from_millis(400));
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(parse_version("v1.10.2"), Some((1, 10, 2)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("2.0.0-beta.1"), Some((2, 0, 0)));
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(is_newer("1.2.0", "1.1.1"));
        assert!(!is_newer("1.1.1", "1.1.1"));
        assert!(!is_newer("1.0.9", "1.1.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }

    #[test]
    fn release_json_and_checksums() {
        let json = br#"{"tag_name":"v9.8.7","html_url":"https://github.com/o/r/releases/tag/v9.8.7","body":"Notes",
            "draft":false,"prerelease":false,"assets":[
            {"name":"VMPC-Setup-9.8.7-x64.exe","browser_download_url":"https://x/setup.exe","size":10},
            {"name":"vmpc-linux-x86_64.tar.gz","browser_download_url":"https://x/linux.tgz","size":20},
            {"name":"SHA256SUMS.txt","browser_download_url":"https://x/sums","size":1}]}"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, "9.8.7");
        assert_eq!(r.sums_url.as_deref(), Some("https://x/sums"));
        let asset = r.asset.unwrap();
        if cfg!(windows) {
            assert_eq!(asset.url, "https://x/setup.exe");
        } else {
            assert_eq!(asset.url, "https://x/linux.tgz");
        }
        let sums = "abc123  VMPC-Setup-9.8.7-x64.exe\nDEF456 *vmpc.exe\n";
        assert_eq!(expected_hash(sums, "VMPC-Setup-9.8.7-x64.exe").as_deref(), Some("abc123"));
        assert_eq!(expected_hash(sums, "vmpc.exe").as_deref(), Some("def456"));
        assert_eq!(expected_hash(sums, "missing"), None);
    }

    #[test]
    fn whats_new_strips_markdown_and_stops_at_downloads() {
        let notes = "## What's new\n\n- **Auto updates** at start\n- `T` toggles tools\n\n## Download\n- installer";
        let lines = whats_new(notes);
        assert_eq!(lines[0], (true, "What's new".to_string()));
        assert_eq!(lines[1], (false, "•  Auto updates at start".to_string()));
        assert_eq!(lines[2], (false, "•  T toggles tools".to_string()));
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn sha256_matches_reference() {
        let path = std::env::temp_dir().join(format!("vmpc_sha_{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(path);
    }

    /// Real request to GitHub; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn latest_release_is_reachable() {
        let json = fetch(&format!("https://api.github.com/repos/{REPO}/releases/latest"), "application/vnd.github+json").unwrap();
        let r = parse_release(&json).unwrap();
        assert!(parse_version(&r.version).is_some());
        assert!(r.asset.is_some() && r.sums_url.is_some());
    }
}
