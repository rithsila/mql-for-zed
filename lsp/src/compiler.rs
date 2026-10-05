use lsp_types::{
    Diagnostic, DiagnosticSeverity, NumberOrString, Position, PublishDiagnosticsParams, Range, Url,
};
use serde::Deserialize;
use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
use walkdir::WalkDir;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub enabled: bool,
    pub script: Option<PathBuf>,
    pub workspace_root: Option<PathBuf>,
}

#[derive(Clone)]
pub struct Save {
    pub job: u64,
    pub source_revision: u64,
    pub uri: Url,
    pub version: i32,
    pub revision: u64,
    pub entry: PathBuf,
}
pub struct Finished {
    pub save: Save,
    pub snapshot: String,
    pub result: Result<CompileResult, String>,
    pub snapshot_current: bool,
}

pub fn is_current(
    done: &Finished,
    docs: &HashMap<Url, (String, i32)>,
    revisions: &HashMap<Url, u64>,
) -> bool {
    revisions.get(&done.save.uri) == Some(&done.save.revision)
        && docs.get(&done.save.uri).map(|(_, v)| *v) == Some(done.save.version)
        && done.snapshot_current
        && done
            .result
            .as_ref()
            .map_or(true, |result| result.source_snapshot == done.snapshot)
}

#[derive(Deserialize)]
pub struct CompileResult {
    schema_version: u32,
    job_id: String,
    source_snapshot: String,
    status: String,
    diagnostics: Vec<RawDiagnostic>,
    message: Option<String>,
}
#[derive(Deserialize)]
struct RawDiagnostic {
    uri: Url,
    severity: String,
    message: String,
    line: u32,
    col: u32,
    code: Option<String>,
}

pub fn entry_point(path: &Path, workspace: &Path) -> Option<PathBuf> {
    match path.extension()?.to_str()? {
        "mq5" => Some(path.to_path_buf()),
        "mqh" => {
            let first = fs::read_to_string(path).ok()?.lines().next()?.to_string();
            let relative = first.strip_prefix("//###<")?.trim_end().strip_suffix('>')?;
            let target = workspace.join(relative);
            (target.is_file()
                && target.starts_with(workspace)
                && target.extension()?.to_str()? == "mq5")
                .then_some(target)
        }
        _ => None,
    }
}

// Identity covers the synced MQL source tree, not just the saved entry point.
pub fn snapshot(workspace: &Path) -> String {
    let mut paths: Vec<_> = WalkDir::new(workspace)
        .max_depth(12)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file()
                && matches!(
                    e.path().extension().and_then(|x| x.to_str()),
                    Some("mq5" | "mqh")
                )
        })
        .map(|e| e.into_path())
        .collect();
    paths.sort();
    let mut hash = DefaultHasher::new();
    for path in paths {
        path.hash(&mut hash);
        match fs::read(path) {
            Ok(bytes) => bytes.hash(&mut hash),
            Err(e) => e.to_string().hash(&mut hash),
        };
    }
    format!("{:016x}", hash.finish())
}

pub fn start(config: Config, workspace: PathBuf) -> (Sender<Save>, Receiver<Finished>) {
    let (tx, rx) = mpsc::channel::<Save>();
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut pending: HashMap<PathBuf, Save> = HashMap::new();
        loop {
            let first = match rx.recv() {
                Ok(save) => save,
                Err(_) => break,
            };
            pending.insert(first.entry.clone(), first);
            let mut deadline = Instant::now() + Duration::from_millis(300);
            loop {
                let timeout = deadline.saturating_duration_since(Instant::now());
                match rx.recv_timeout(timeout) {
                    Ok(save) => {
                        pending.insert(save.entry.clone(), save);
                        deadline = Instant::now() + Duration::from_millis(300);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            for (_, save) in pending.drain() {
                let before = snapshot(&workspace);
                let result = run(&config, &workspace, &save, &before);
                let snapshot_current = snapshot(&workspace) == before;
                if done_tx
                    .send(Finished {
                        save,
                        snapshot: before,
                        result,
                        snapshot_current,
                    })
                    .is_err()
                {
                    return;
                }
            }
        }
    });
    (tx, done_rx)
}

fn run(
    config: &Config,
    workspace: &Path,
    save: &Save,
    snapshot: &str,
) -> Result<CompileResult, String> {
    let script = config
        .script
        .as_ref()
        .ok_or("compilerCheck.script is required")?;
    let job_id = save.job.to_string();
    let mut child = Command::new("bash")
        .arg(script)
        .args([
            "--check",
            "--json",
            "--job-id",
            &job_id,
            "--snapshot",
            snapshot,
        ])
        .arg(save.uri.to_file_path().map_err(|_| "invalid file URI")?)
        .env("MQL_WORKSPACE_ROOT", workspace)
        .env("MQL_DEPLOY", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start syntax check: {e}"))?;
    // Drain both pipes while the child runs: a large compiler log must not block a check forever.
    fn drain<R: Read + Send + 'static>(mut reader: R) -> thread::JoinHandle<Vec<u8>> {
        thread::spawn(move || {
            let mut limited = (&mut reader).take(4 * 1024 * 1024);
            let mut bytes = Vec::new();
            let _ = limited.read_to_end(&mut bytes);
            let _ = io::copy(&mut reader, &mut io::sink());
            bytes
        })
    }
    let stdout = drain(child.stdout.take().unwrap());
    let stderr = drain(child.stderr.take().unwrap());
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(60) => {
                thread::sleep(Duration::from_millis(30))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("syntax check timed out after 60 seconds; remote state unknown".into());
            }
            Err(e) => return Err(format!("syntax check failed: {e}")),
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let out = stdout.join().map_err(|_| "compiler output reader failed")?;
    let err = stderr.join().map_err(|_| "compiler error reader failed")?;
    let value: CompileResult = serde_json::from_slice(&out).map_err(|e| {
        format!(
            "invalid compiler JSON: {e}; stderr: {}",
            String::from_utf8_lossy(&err)
        )
    })?;
    if value.schema_version != 1 || value.job_id != job_id || value.source_snapshot != snapshot {
        return Err("compiler result identity/schema mismatch".into());
    }
    if !matches!(
        (value.status.as_str(), status.code()),
        ("success", Some(0))
            | ("compiler_errors", Some(1))
            | ("ssh_failure" | "launch_failure" | "log_failure", Some(2))
    ) {
        return Err("compiler result status/exit code mismatch".into());
    }
    Ok(value)
}

// Verified with MetaEditor on Windows: the one-based column after /*🐈é*/
// is 21 (UTF-16 units), not 20 scalars or 24 UTF-8 bytes. LSP uses zero-based UTF-16.
fn position(line: u32, col: u32) -> Position {
    Position::new(line.saturating_sub(1), col.saturating_sub(1))
}

pub struct Publications {
    by_entry: HashMap<PathBuf, HashMap<Url, Vec<Diagnostic>>>,
}
impl Publications {
    pub fn new() -> Self {
        Self {
            by_entry: HashMap::new(),
        }
    }
    pub fn apply(
        &mut self,
        entry: PathBuf,
        result: CompileResult,
    ) -> Option<Vec<PublishDiagnosticsParams>> {
        if !matches!(result.status.as_str(), "success" | "compiler_errors") {
            eprintln!(
                "mql-lsp: syntax check {}: {}",
                result.status,
                result.message.unwrap_or_default()
            );
            return None;
        }
        let mut next: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        for raw in result.diagnostics {
            let start = position(raw.line, raw.col);
            let severity = match raw.severity.as_str() {
                "error" => DiagnosticSeverity::ERROR,
                "warning" => DiagnosticSeverity::WARNING,
                _ => continue,
            };
            next.entry(raw.uri).or_default().push(Diagnostic {
                range: Range::new(start, start),
                severity: Some(severity),
                code: raw.code.map(NumberOrString::String),
                source: Some("MetaEditor".into()),
                message: raw.message,
                ..Default::default()
            });
        }
        let old = self.by_entry.insert(entry, next);
        let uris: HashSet<Url> = old
            .into_iter()
            .flat_map(|m| m.into_keys())
            .chain(self.by_entry.values().flat_map(|m| m.keys().cloned()))
            .collect();
        Some(
            uris.into_iter()
                .map(|uri| PublishDiagnosticsParams {
                    diagnostics: self
                        .by_entry
                        .values()
                        .filter_map(|m| m.get(&uri))
                        .flat_map(|v| v.iter().cloned())
                        .collect(),
                    uri,
                    version: None,
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metaeditor_columns_are_one_based_utf16() {
        // Live Windows fixture: /*🐈é*/ int value = MissingM1Symbol;
        // MetaEditor reports (3,21); the symbol starts at UTF-16 offset 20.
        assert_eq!(position(3, 21), Position::new(2, 20));
        assert_eq!(position(1, 1), Position::new(0, 0));
        assert_eq!(position(0, 0), Position::new(0, 0));
    }
    #[test]
    fn stale_versions_and_snapshots_are_rejected() {
        let uri = Url::from_file_path("/tmp/test.mq5").unwrap();
        let save = Save {
            job: 1,
            source_revision: 1,
            uri: uri.clone(),
            version: 3,
            revision: 5,
            entry: PathBuf::from("entry"),
        };
        let mut docs = HashMap::from([(uri.clone(), (String::new(), 3))]);
        let mut revisions = HashMap::from([(uri.clone(), 5)]);
        let mut done = Finished {
            save,
            snapshot: "hash".into(),
            result: Err("unused".into()),
            snapshot_current: true,
        };
        assert!(is_current(&done, &docs, &revisions));
        done.snapshot_current = false;
        assert!(!is_current(&done, &docs, &revisions));
        done.snapshot_current = true;
        docs.get_mut(&uri).unwrap().1 = 4;
        assert!(!is_current(&done, &docs, &revisions));
        docs.get_mut(&uri).unwrap().1 = 3;
        revisions.insert(uri, 6);
        assert!(!is_current(&done, &docs, &revisions));
    }
    #[test]
    fn entry_point_marker_and_snapshot_change() {
        let root = std::env::temp_dir().join(format!("mql-entry-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let ea = root.join("EA.mq5");
        let header = root.join("header.mqh");
        fs::write(&ea, "void OnStart() {}\n").unwrap();
        fs::write(&header, "//###<EA.mq5>\n").unwrap();
        assert_eq!(entry_point(&header, &root), Some(ea.clone()));
        let first = snapshot(&root);
        fs::write(&header, "//###<EA.mq5>\nint x;\n").unwrap();
        assert_ne!(snapshot(&root), first);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn clearing_and_infrastructure_failure() {
        let mut state = Publications::new();
        let uri = Url::from_file_path("/tmp/test.mq5").unwrap();
        let raw = RawDiagnostic {
            uri: uri.clone(),
            severity: "error".into(),
            message: "bad".into(),
            line: 1,
            col: 1,
            code: None,
        };
        let result = |status: &str, diagnostics| CompileResult {
            schema_version: 1,
            job_id: "x".into(),
            source_snapshot: "s".into(),
            status: status.into(),
            diagnostics,
            message: None,
        };
        assert_eq!(
            state
                .apply(PathBuf::from("entry"), result("compiler_errors", vec![raw]))
                .unwrap()[0]
                .diagnostics
                .len(),
            1
        );
        assert!(state
            .apply(PathBuf::from("entry"), result("ssh_failure", vec![]))
            .is_none());
        let updates = state
            .apply(PathBuf::from("entry"), result("success", vec![]))
            .unwrap();
        assert_eq!(
            updates
                .iter()
                .find(|p| p.uri == uri)
                .unwrap()
                .diagnostics
                .len(),
            0
        );
    }
}
