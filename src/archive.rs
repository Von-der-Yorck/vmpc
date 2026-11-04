//! Archives (ZIP, 7Z, RAR, TAR.*, ISO, CAB, CPIO, XAR, LHA, DEB, RPM, comic books …)
//! through libarchive's `bsdtar`: the `tar.exe` built into Windows 10+, or `bsdtar`
//! from the libarchive-tools package on Linux. VMPC lists the contents, extracts
//! them, repacks into another archive format, and plays media inside.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::ffmpeg::{command, system_tool};
use crate::jobs::Job;
use crate::util;

/// Output archive formats for "repack".
pub const REPACK_FORMATS: &[(&str, &str)] = &[
    ("ZIP", "zip"),
    ("7-Zip", "7z"),
    ("TAR.GZ", "tar.gz"),
    ("TAR.XZ", "tar.xz"),
    ("TAR.ZST", "tar.zst"),
    ("TAR.BZ2", "tar.bz2"),
    ("TAR", "tar"),
    ("ISO image", "iso"),
];

/// The libarchive `tar`, if this system has one.
pub fn tool() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(windows) { vec![system_tool("tar")] } else { vec!["bsdtar".into(), "tar".into()] };
    candidates.into_iter().find(|t| {
        command(t)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("libarchive"))
            .unwrap_or(false)
    })
}

fn missing() -> String {
    if cfg!(windows) {
        "The Windows archive tool (tar.exe) is missing.".into()
    } else {
        "Install bsdtar to open archives, e.g. sudo apt install libarchive-tools".into()
    }
}

/// File names inside the archive (folders left out).
pub fn list(path: &Path) -> Result<Vec<String>, String> {
    let tool = tool().ok_or_else(missing)?;
    let out = command(&tool).arg("-tf").arg(path).stdout(Stdio::piped()).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Can't read this archive: {}", err.lines().last().unwrap_or("").trim()));
    }
    // Some formats (ISO) list folders without a trailing slash; the verbose listing marks
    // them with a leading `d`, line for line in the same order.
    let verbose = command(&tool).arg("-tvf").arg(path).stdout(Stdio::piped()).stderr(Stdio::null()).output().ok();
    let kinds: Vec<bool> = verbose
        .map(|v| String::from_utf8_lossy(&v.stdout).lines().map(|l| l.starts_with('d')).collect())
        .unwrap_or_default();
    let names = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines: Vec<&str> = names.lines().collect();
    let same_shape = kinds.len() == lines.len();
    Ok(lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !(same_shape && kinds[*i]))
        .map(|(_, l)| l.trim_start_matches("./").to_string())
        .filter(|l| !l.is_empty() && l != "." && !l.ends_with('/'))
        .collect())
}

/// Archive name without its (possibly compound) extension: `photos.tar.gz` → `photos`.
pub fn stem(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "archive".into());
    let lower = name.to_lowercase();
    for suffix in [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tar.lz"] {
        if lower.ends_with(suffix) {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    Path::new(&name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(name)
}

/// Extracts everything into a new folder next to the archive (or into `dir`).
pub fn extract_job(path: &Path, dir: Option<&Path>) -> Result<Job, String> {
    let tool = tool().ok_or_else(missing)?;
    let parent = dir.map(PathBuf::from).or_else(|| path.parent().map(PathBuf::from)).unwrap_or_else(|| util::default_output_dir("Downloads"));
    let out = util::unique_dir(&parent, &stem(path));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let args: Vec<OsString> = vec!["-xf".into(), path.into(), "-C".into(), out.clone().into()];
    let mut job = Job::new(format!("Extract {}", util::display_name(&path.to_string_lossy())), out, args, None);
    job.tool = Some(tool);
    Ok(job)
}

/// Rewrites the archive in another format (`ext` from [`REPACK_FORMATS`]).
pub fn repack_job(path: &Path, ext: &str, dir: Option<&Path>) -> Result<Job, String> {
    let tool = tool().ok_or_else(missing)?;
    let parent = dir.map(PathBuf::from).or_else(|| path.parent().map(PathBuf::from)).unwrap_or_else(|| util::default_output_dir("Downloads"));
    let output = util::unique_path(&parent, &stem(path), ext);
    // `-a` picks format and compression from the output name; `@archive` copies entries.
    let mut at = OsString::from("@");
    at.push(path);
    let args: Vec<OsString> = vec!["-a".into(), "-cf".into(), output.clone().into(), at];
    let label = REPACK_FORMATS.iter().find(|f| f.1 == ext).map(|f| f.0).unwrap_or(ext);
    let mut job = Job::new(format!("{} → {label}", util::display_name(&path.to_string_lossy())), output, args, None);
    job.tool = Some(tool);
    Ok(job)
}

/// Extracts into a private temp folder and returns the files (to play media inside).
pub fn extract_to_temp(path: &Path) -> Result<Vec<PathBuf>, String> {
    let tool = tool().ok_or_else(missing)?;
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let dir = std::env::temp_dir().join("vmpc-archives").join(format!("{}-{id}", stem(path)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ok = command(&tool).arg("-xf").arg(path).arg("-C").arg(&dir).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !ok.map(|s| s.success()).unwrap_or(false) {
        return Err("Extracting the archive failed.".into());
    }
    let mut files = Vec::new();
    walk(&dir, &mut files);
    files.sort_by(|a, b| util::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    Ok(files)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{testing, Status};

    #[test]
    fn stems() {
        assert_eq!(stem(Path::new("/x/photos.tar.gz")), "photos");
        assert_eq!(stem(Path::new("/x/My.Backup.zip")), "My.Backup");
    }

    #[test]
    fn list_extract_repack_round_trip() {
        if tool().is_none() {
            return; // no libarchive tar on this machine
        }
        let dir = testing::scratch_dir("archive");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        std::fs::write(src.join("sub").join("b.txt"), "world").unwrap();
        let zip = dir.join("pack.zip");
        let ok = command(tool().unwrap()).arg("-a").arg("-cf").arg(&zip).arg("-C").arg(&src).arg(".").status().unwrap();
        assert!(ok.success());

        let mut names = list(&zip).unwrap();
        names.sort();
        assert_eq!(names, vec!["a.txt".to_string(), "sub/b.txt".to_string()]);

        for ext in ["7z", "tar.gz", "tar.xz", "tar.zst", "iso"] {
            let job = testing::run(repack_job(&zip, ext, None).unwrap());
            assert_eq!(job.status, Status::Done, "{ext}: {:?}", job.status);
            let mut again = list(&job.output).unwrap();
            again.sort();
            assert_eq!(again.len(), 2, "{ext} has {again:?}");
        }

        let job = testing::run(extract_job(&zip, None).unwrap());
        assert_eq!(job.status, Status::Done);
        assert_eq!(std::fs::read_to_string(job.output.join("sub").join("b.txt")).unwrap(), "world");

        let files = extract_to_temp(&zip).unwrap();
        assert_eq!(files.len(), 2);
        let _ = std::fs::remove_dir_all(files[0].parent().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
