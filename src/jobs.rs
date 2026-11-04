//! Background conversion queue. Jobs run one at a time (keeps the machine responsive)
//! in a worker thread; progress comes from ffmpeg's `-progress` key/value output.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::ffmpeg;
use crate::util;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Queued,
    Running,
    Done,
    Failed(String),
    Cancelled,
}

/// Work to do after ffmpeg finished successfully.
#[derive(Clone, Debug)]
pub enum Post {
    None,
    /// Renumber an extracted image sequence so the last frame becomes frame 1.
    ReverseNumbering { dir: PathBuf, prefix: String, ext: String },
}

#[derive(Clone, Debug)]
pub struct Job {
    pub id: u64,
    pub title: String,
    pub output: PathBuf,
    pub args: Vec<OsString>,
    /// Expected output duration in seconds (for the progress bar).
    pub total: Option<f64>,
    pub post: Post,
    /// Temporary files removed when the job ends.
    pub temp: Vec<PathBuf>,
    pub progress: f32,
    pub status: Status,
    pub started: Option<Instant>,
    pub elapsed: f64,
    pub speed: String,
    /// Run this program instead of ffmpeg (archives use the system `tar`).
    pub tool: Option<PathBuf>,
}

impl Job {
    pub fn new(title: String, output: PathBuf, args: Vec<OsString>, total: Option<f64>) -> Self {
        Job {
            id: 0,
            title,
            output,
            args,
            total,
            post: Post::None,
            temp: Vec::new(),
            progress: 0.0,
            status: Status::Queued,
            started: None,
            elapsed: 0.0,
            speed: String::new(),
            tool: None,
        }
    }
}

#[derive(Default)]
struct Shared {
    jobs: Mutex<Vec<Job>>,
    cancelled: Mutex<HashSet<u64>>,
    child: Mutex<Option<(u64, Child)>>,
}

pub struct Queue {
    shared: Arc<Shared>,
    wake: Sender<()>,
    next_id: u64,
}

impl Queue {
    pub fn new(ctx: egui::Context) -> Self {
        let shared = Arc::new(Shared::default());
        let (wake, rx) = mpsc::channel::<()>();
        let worker_shared = shared.clone();
        std::thread::Builder::new()
            .name("job-worker".into())
            .spawn(move || loop {
                let next = {
                    let mut jobs = worker_shared.jobs.lock().unwrap();
                    jobs.iter_mut().find(|j| j.status == Status::Queued).map(|j| {
                        j.status = Status::Running;
                        j.started = Some(Instant::now());
                        j.clone()
                    })
                };
                match next {
                    Some(job) => {
                        run_job(&worker_shared, job, &ctx);
                        ctx.request_repaint();
                    }
                    None => {
                        if rx.recv().is_err() {
                            break;
                        }
                    }
                }
            })
            .expect("spawn job worker");
        Queue { shared, wake, next_id: 1 }
    }

    pub fn push(&mut self, mut job: Job) {
        job.id = self.next_id;
        self.next_id += 1;
        self.shared.jobs.lock().unwrap().push(job);
        let _ = self.wake.send(());
    }

    pub fn snapshot(&self) -> Vec<Job> {
        let mut jobs = self.shared.jobs.lock().unwrap().clone();
        for j in &mut jobs {
            if j.status == Status::Running {
                j.elapsed = j.started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
            }
        }
        jobs
    }

    pub fn is_busy(&self) -> bool {
        self.shared.jobs.lock().unwrap().iter().any(|j| matches!(j.status, Status::Queued | Status::Running))
    }

    pub fn cancel(&self, id: u64) {
        self.shared.cancelled.lock().unwrap().insert(id);
        {
            let mut jobs = self.shared.jobs.lock().unwrap();
            if let Some(j) = jobs.iter_mut().find(|j| j.id == id && j.status == Status::Queued) {
                j.status = Status::Cancelled;
            }
        }
        if let Some((cid, child)) = self.shared.child.lock().unwrap().as_mut() {
            if *cid == id {
                let _ = child.kill();
            }
        }
    }

    pub fn clear_finished(&self) {
        self.shared
            .jobs
            .lock()
            .unwrap()
            .retain(|j| matches!(j.status, Status::Queued | Status::Running));
    }
}

fn update(shared: &Shared, id: u64, f: impl FnOnce(&mut Job)) {
    if let Some(j) = shared.jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
        f(j);
    }
}

fn run_job(shared: &Shared, job: Job, ctx: &egui::Context) {
    let result = execute(shared, &job, ctx);
    let cancelled = shared.cancelled.lock().unwrap().contains(&job.id);
    for t in &job.temp {
        let _ = std::fs::remove_file(t);
    }
    let status = match result {
        _ if cancelled => {
            if job.output.is_file() {
                let _ = std::fs::remove_file(&job.output);
            }
            Status::Cancelled
        }
        Ok(()) => match apply_post(&job.post) {
            Ok(()) => Status::Done,
            Err(e) => Status::Failed(e),
        },
        Err(e) => Status::Failed(e),
    };
    update(shared, job.id, |j| {
        if status == Status::Done {
            j.progress = 1.0;
        }
        j.elapsed = j.started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
        j.status = status;
    });
}

fn execute(shared: &Shared, job: &Job, ctx: &egui::Context) -> Result<(), String> {
    let mut cmd = match &job.tool {
        // Other tools have no progress protocol; the bar simply runs until done.
        Some(tool) => ffmpeg::command(tool),
        None => {
            let mut c = ffmpeg::ffmpeg_cmd().ok_or("FFmpeg not found")?;
            c.args(["-y", "-v", "error", "-nostats", "-progress", "pipe:1"]);
            c
        }
    };
    cmd.args(&job.args);
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", job.tool.as_ref().map(|t| t.display().to_string()).unwrap_or("ffmpeg".into())))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    *shared.child.lock().unwrap() = Some((job.id, child));
    if shared.cancelled.lock().unwrap().contains(&job.id) {
        if let Some((_, c)) = shared.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }

    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let mut last_repaint = Instant::now();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Some((key, value)) = line.split_once('=') else { continue };
        match key {
            "out_time_us" | "out_time_ms" => {
                if let (Ok(us), Some(total)) = (value.trim().parse::<f64>(), job.total) {
                    let frac = (us / 1e6 / total.max(0.001)).clamp(0.0, 0.999) as f32;
                    update(shared, job.id, |j| j.progress = frac);
                }
            }
            "speed" => {
                let v = value.trim().to_string();
                update(shared, job.id, |j| j.speed = v);
            }
            _ => {}
        }
        if last_repaint.elapsed().as_millis() > 100 {
            ctx.request_repaint();
            last_repaint = Instant::now();
        }
    }

    let status = shared.child.lock().unwrap().take().map(|(_, mut c)| c.wait());
    let err_text = err_reader.join().unwrap_or_default();
    match status {
        Some(Ok(s)) if s.success() => Ok(()),
        _ => {
            let msg = err_text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("ffmpeg failed")
                .trim()
                .to_string();
            Err(msg)
        }
    }
}

fn apply_post(post: &Post) -> Result<(), String> {
    match post {
        Post::None => Ok(()),
        Post::ReverseNumbering { dir, prefix, ext } => {
            let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    name.starts_with(prefix.as_str()) && name.ends_with(&format!(".{ext}"))
                })
                .collect();
            files.sort_by(|a, b| util::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
            // Two passes so names never collide.
            let mut temps = Vec::with_capacity(files.len());
            for (i, f) in files.iter().enumerate() {
                let tmp = dir.join(format!(".vmpc_tmp_{i}.{ext}"));
                std::fs::rename(f, &tmp).map_err(|e| e.to_string())?;
                temps.push(tmp);
            }
            let n = temps.len();
            for (i, tmp) in temps.iter().enumerate() {
                let target = dir.join(format!("{prefix}{:06}.{ext}", n - i));
                std::fs::rename(tmp, target).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_numbering_renames_backwards() {
        let dir = std::env::temp_dir().join(format!("vmpc_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 1..=3 {
            std::fs::write(dir.join(format!("f_{i:06}.png")), format!("{i}")).unwrap();
        }
        apply_post(&Post::ReverseNumbering { dir: dir.clone(), prefix: "f_".into(), ext: "png".into() }).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("f_000001.png")).unwrap(), "3");
        assert_eq!(std::fs::read_to_string(dir.join("f_000003.png")).unwrap(), "1");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// Test helpers that run jobs through the real queue and FFmpeg.
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::OnceLock;

    /// A 3 second 320×240 test clip with a 440 Hz tone, generated once per test run.
    pub fn sample_video() -> Option<PathBuf> {
        static SAMPLE: OnceLock<Option<PathBuf>> = OnceLock::new();
        SAMPLE
            .get_or_init(|| {
                let path = std::env::temp_dir().join("vmpc_test_sample.mp4");
                let ok = ffmpeg::ffmpeg_cmd()?
                    .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=3"])
                    .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=3"])
                    .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
                    .arg(&path)
                    .status()
                    .ok()?
                    .success();
                ok.then_some(path)
            })
            .clone()
    }

    pub fn run(job: Job) -> Job {
        let mut queue = Queue::new(egui::Context::default());
        queue.push(job);
        let deadline = Instant::now() + std::time::Duration::from_secs(120);
        loop {
            let snap = queue.snapshot();
            let j = snap.into_iter().next().unwrap();
            if !matches!(j.status, Status::Queued | Status::Running) || Instant::now() > deadline {
                return j;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    pub fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vmpc_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
