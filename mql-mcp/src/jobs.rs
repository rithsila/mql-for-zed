use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Preparing,
    Compiling,
    Running,
    Collecting,
    Completed,
    Failed,
    CancelRequested,
    Cancelled,
    TimedOut,
    RemoteStateUnknown,
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobState::Completed
                | JobState::Failed
                | JobState::Cancelled
                | JobState::TimedOut
                | JobState::RemoteStateUnknown
        )
    }
}

/// Server-side bounds so one client cannot grow state without limit.
pub const MAX_ACTIVE_JOBS: usize = 8;
pub const MAX_RESULT_BYTES: usize = 65536;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub tool: String,
    pub state: JobState,
    pub created_at: u64,
    pub updated_at: u64,
    pub result: Option<String>,
    pub run_id: Option<String>,
    pub run_dir: Option<String>,
    pub pid: Option<u32>,
    pub worker: Option<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn truncate_result(text: &str) -> String {
    if text.len() <= MAX_RESULT_BYTES {
        return text.to_string();
    }
    let cut = text.floor_char_boundary(MAX_RESULT_BYTES);
    format!(
        "{}...(truncated {} bytes of output)",
        &text[..cut],
        text.len() - cut
    )
}

fn persist(path: &Path, job: &Job) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(serde_json::to_string(job)?.as_bytes())?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}
/// Jobs are persisted before launch and survive a controller restart.
/// On open(), every non-terminal job is reconciled to RemoteStateUnknown:
/// the child died with the controller, but the worker (or a remote MT5
/// terminal) may still be working. Never retry those blindly.
pub struct JobManager {
    dir: PathBuf,
    jobs: Mutex<HashMap<String, Job>>,
    reconciled: Mutex<Vec<String>>,
}

impl JobManager {
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let mut jobs = HashMap::new();
        let mut reconciled = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut job) = serde_json::from_str::<Job>(&text) else {
                continue;
            };
            if !job.state.is_terminal() {
                job.state = JobState::RemoteStateUnknown;
                job.result = Some(
                    "controller restarted before this job reached a terminal state; the worker may still hold the terminal: inspect remote state before launching anything else on it".to_string(),
                );
                reconciled.push(job.id.clone());
                persist(&path, &job)?;
            }
            jobs.insert(job.id.clone(), job);
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            jobs: Mutex::new(jobs),
            reconciled: Mutex::new(reconciled),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn reconciled_at_startup(&self) -> Vec<String> {
        self.reconciled.lock().unwrap().clone()
    }

    pub fn create(&self, tool: &str, worker: Option<String>) -> io::Result<Job> {
        {
            let guard = self.jobs.lock().unwrap();
            let active = guard.values().filter(|j| !j.state.is_terminal()).count();
            if active >= MAX_ACTIVE_JOBS {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "queue full: {} active jobs (max {})",
                        active, MAX_ACTIVE_JOBS
                    ),
                ));
            }
        }
        let job = Job {
            id: Uuid::new_v4().to_string(),
            tool: tool.to_string(),
            state: JobState::Queued,
            created_at: now_secs(),
            updated_at: now_secs(),
            result: None,
            run_id: None,
            run_dir: None,
            pid: None,
            worker,
        };
        persist(&self.file(&job.id), &job)?;
        self.jobs
            .lock()
            .unwrap()
            .insert(job.id.clone(), job.clone());
        Ok(job)
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(id.to_string() + ".json")
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.jobs.lock().unwrap().get(id).cloned()
    }

    pub fn list(&self) -> Vec<Job> {
        let mut v: Vec<Job> = self.jobs.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v
    }

    pub fn active_count(&self) -> usize {
        self.jobs
            .lock()
            .unwrap()
            .values()
            .filter(|j| !j.state.is_terminal())
            .count()
    }

    pub fn update<F: FnOnce(&mut Job)>(
        &self,
        id: &str,
        expected: Option<JobState>,
        f: F,
    ) -> io::Result<Job> {
        let mut guard = self.jobs.lock().unwrap();
        let job = guard
            .get_mut(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "job not found"))?;
        if job.state.is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("job already terminal ({:?})", job.state),
            ));
        }
        if let Some(exp) = expected {
            if job.state != exp {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("unexpected state {:?}, expected {:?}", job.state, exp),
                ));
            }
        }
        f(job);
        job.updated_at = now_secs();
        let snapshot = job.clone();
        drop(guard);
        persist(&self.file(id), &snapshot)?;
        Ok(snapshot)
    }

    /// Terminal transition. Result storage is separated from state: failing a
    /// job stores its result while the state stays Failed (fixes the old
    /// complete_job-on-failure bug).
    pub fn finish(&self, id: &str, state: JobState, result: Option<String>) -> io::Result<Job> {
        assert!(state.is_terminal(), "finish requires a terminal state");
        let mut guard = self.jobs.lock().unwrap();
        let job = guard
            .get_mut(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "job not found"))?;
        if job.state.is_terminal() {
            return Ok(job.clone());
        }
        job.state = if job.state == JobState::CancelRequested
            && matches!(
                state,
                JobState::Failed | JobState::Completed | JobState::TimedOut
            ) {
            JobState::Cancelled
        } else {
            state
        };
        job.result = result.map(|r| truncate_result(&r));
        job.updated_at = now_secs();
        let snapshot = job.clone();
        drop(guard);
        persist(&self.file(id), &snapshot)?;
        Ok(snapshot)
    }

    pub fn request_cancel(&self, id: &str) -> io::Result<Job> {
        self.update(id, None, |job| job.state = JobState::CancelRequested)
    }
}
pub fn pid_alive(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// One lock file per worker (broker VM install). Prevents two runs sharing
/// the same MT5 data dir, tester agent, and Common-Files export names.
pub struct Locks {
    dir: PathBuf,
}

impl Locks {
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    fn sanitize(worker: &str) -> String {
        worker
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    }

    fn path(&self, worker: &str) -> PathBuf {
        self.dir.join(Self::sanitize(worker) + ".lock")
    }

    /// Ok(()) when acquired. Err(owner description) when a live job holds it.
    /// A lock whose recorded pid is dead is stale and gets reclaimed.
    pub fn acquire(&self, worker: &str, job_id: &str, pid: u32) -> Result<(), String> {
        let path = self.path(worker);
        if let Ok(text) = fs::read_to_string(&path) {
            let mut parts = text.split_whitespace();
            let held_by = parts.next().unwrap_or("?").to_string();
            let held_pid: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            if held_pid > 0 && pid_alive(held_pid) && held_by != job_id {
                return Err(format!(
                    "worker {} is busy: job {} (pid {})",
                    worker, held_by, held_pid
                ));
            }
        }
        let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
        f.write_all(format!("{} {}", job_id, pid).as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn release(&self, worker: &str, job_id: &str) {
        let path = self.path(worker);
        if let Ok(text) = fs::read_to_string(&path) {
            if text.split_whitespace().next() == Some(job_id) {
                let _ = fs::remove_file(&path);
            }
        }
    }

    pub fn status(&self) -> Vec<(String, String, bool)> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let Some(worker) = name.strip_suffix(".lock") else {
                    continue;
                };
                let text = fs::read_to_string(entry.path()).unwrap_or_default();
                let mut parts = text.split_whitespace();
                let job = parts.next().unwrap_or("?").to_string();
                let pid: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
                out.push((worker.to_string(), job, pid > 0 && pid_alive(pid)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_mgr(tag: &str) -> (JobManager, PathBuf) {
        let dir = std::env::temp_dir().join(format!("zedmql-jobs-{}-{}", tag, Uuid::new_v4()));
        (JobManager::open(&dir).unwrap(), dir)
    }

    #[test]
    fn create_persists_and_survives_reload() {
        let (mgr, dir) = temp_mgr("persist");
        let job = mgr.create("mql_backtest", None).unwrap();
        assert!(dir.join(job.id.clone() + ".json").is_file());
        // Simulated restart: the job is still active -> must be reconciled to
        // RemoteStateUnknown, never silently to Completed.
        let re = JobManager::open(&dir).unwrap();
        let j = re.get(&job.id).unwrap();
        assert_eq!(j.state, JobState::RemoteStateUnknown);
        assert_eq!(re.reconciled_at_startup(), vec![job.id.clone()]);
        assert_eq!(re.active_count(), 0, "reconciled jobs are terminal");
    }

    #[test]
    fn failure_stays_failed_with_result() {
        let (mgr, _dir) = temp_mgr("fail");
        let job = mgr.create("mql_compile", None).unwrap();
        mgr.update(&job.id, None, |j| j.state = JobState::Running)
            .unwrap();
        mgr.finish(&job.id, JobState::Failed, Some("boom".into()))
            .unwrap();
        let j = mgr.get(&job.id).unwrap();
        assert_eq!(j.state, JobState::Failed);
        assert_eq!(j.result.as_deref(), Some("boom"));
        // terminal states are never overwritten
        mgr.finish(&job.id, JobState::Completed, Some("late".into()))
            .unwrap();
        assert_eq!(mgr.get(&job.id).unwrap().state, JobState::Failed);
    }

    #[test]
    fn cancel_beats_runner_success() {
        let (mgr, _dir) = temp_mgr("cancel");
        let job = mgr.create("mql_backtest", None).unwrap();
        mgr.request_cancel(&job.id).unwrap();
        let j = mgr
            .finish(&job.id, JobState::Failed, Some("killed".into()))
            .unwrap();
        assert_eq!(j.state, JobState::Cancelled);
    }

    #[test]
    fn locks_reject_live_and_reclaim_stale() {
        let dir = std::env::temp_dir().join(format!("zedmql-locks-{}", Uuid::new_v4()));
        let locks = Locks::open(&dir).unwrap();
        let me = std::process::id();
        locks.acquire("vm-mqlvm", "job-a", me).unwrap();
        let err = locks.acquire("vm-mqlvm", "job-b", 4242).unwrap_err();
        assert!(err.contains("busy"), "{}", err);
        locks.release("vm-mqlvm", "job-a");
        locks.acquire("vm-mqlvm", "job-b", 4242).unwrap();
        // owner pid dead -> stale lock reclaimed
        locks.acquire("vm-mqlvm", "job-c", 4242).unwrap();
        let st = locks.status();
        assert_eq!(st.len(), 1);
        assert!(!st[0].2, "dead pid must not report alive");
    }
}
