mod jobs;
mod tools;

use mcp_server::{
    ByteTransport, Server,
    router::{CapabilitiesBuilder, Router, RouterService},
};
use mcp_spec::content::Content;
use mcp_spec::handler::{PromptError, ResourceError, ToolError};
use mcp_spec::prompt::Prompt;
use mcp_spec::protocol::ServerCapabilities;
use mcp_spec::resource::Resource;
use mcp_spec::tool::Tool;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::{future::Future, pin::Pin, time::Duration};
use tokio::process::Command;

use crate::jobs::{JobManager, JobState, Locks};

const SCRIPTS_DIR: &str = "scripts";

#[derive(Clone, Debug)]
struct Cfg {
    vm_host: String,
    vm_terminal: String,
    vm_mql5_root: Option<String>,
}

#[derive(Clone)]
struct MqlRouter {
    jobs: Arc<JobManager>,
    locks: Arc<Locks>,
    ws: Arc<PathBuf>,
    cfg: Arc<Cfg>,
    roots: Arc<Vec<PathBuf>>,
}

fn chr_single() -> char {
    char::from(39)
}

async fn load_cfg(ws: &Path) -> Cfg {
    let mut out = Cfg {
        vm_host: "mqlvm".to_string(),
        vm_terminal: "C:/Program Files/MetaTrader 5/terminal64.exe".to_string(),
        vm_mql5_root: None,
    };
    let wsd = ws.display().to_string();
    if wsd.contains(chr_single()) {
        return out;
    }
    let tpl = r##"CFG="$HOME/.config/zed-mql/env"; if [ -n "$MQL_CONFIG" ]; then CFG="$MQL_CONFIG"; fi; [ -f "$CFG" ] && . "$CFG"; [ -f 'WSDIR/.zed/mql-tester.env' ] && . 'WSDIR/.zed/mql-tester.env'; echo "HOST=$MQL_VM_HOST"; echo "TERM=$MQL_VM_TERMINAL"; echo "MQL5ROOT=$MQL_VM_MQL5_ROOT""##;
    let script = tpl.replace("WSDIR", &wsd);
    if let Ok(o) = Command::new("bash").arg("-c").arg(script).output().await {
        let text = String::from_utf8_lossy(&o.stdout).to_string();
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("HOST=") {
                if !v.is_empty() {
                    out.vm_host = v.to_string();
                }
            } else if let Some(v) = line.strip_prefix("TERM=") {
                if !v.is_empty() {
                    out.vm_terminal = v.to_string();
                }
            } else if let Some(v) = line.strip_prefix("MQL5ROOT=") {
                if !v.is_empty() {
                    out.vm_mql5_root = Some(v.to_string());
                }
            }
        }
    }
    out
}

fn allowed_roots(ws: &Path) -> Vec<PathBuf> {
    let mut roots = vec![ws.to_path_buf()];
    match std::env::var("MQL_ALLOWED_ROOTS") {
        Ok(list) => {
            for p in list.split(':') {
                if let Ok(c) = std::fs::canonicalize(p) {
                    roots.push(c);
                }
            }
        }
        Err(_) => {
            if let Ok(home) = std::env::var("HOME") {
                if let Ok(c) = std::fs::canonicalize(Path::new(&home).join("Projects")) {
                    roots.push(c);
                }
            }
        }
    }
    roots
}

fn resolve_inside(
    raw: &str,
    ws: &Path,
    roots: &[PathBuf],
    exts: &[&str],
) -> Result<PathBuf, String> {
    if raw.is_empty() {
        return Err("empty path".into());
    }
    let p = Path::new(raw);
    for c in p.components() {
        if matches!(c, Component::ParentDir) {
            return Err("path traversal ('..') is not allowed".into());
        }
    }
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        ws.join(p)
    };
    let canon = std::fs::canonicalize(&abs).map_err(|_| format!("not found: {}", raw))?;
    if !roots.iter().any(|r| canon.starts_with(r)) {
        return Err(format!(
            "{} is outside approved workspace roots",
            canon.display()
        ));
    }
    let ext = canon
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    if !exts.contains(&ext.as_str()) {
        return Err(format!("file type not allowed: .{}", ext));
    }
    Ok(canon)
}

fn validate_run_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id == "." || id == ".." || id.contains('/') || id.contains("..") {
        return Err(format!("invalid run id: {}", id));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!("invalid characters in run id: {}", id));
    }
    Ok(())
}

fn v_date(s: &str) -> bool {
    let b: Vec<char> = s.chars().collect();
    if b.len() != 10 || b[4] != '.' || b[7] != '.' {
        return false;
    }
    let y = s[0..4].parse::<u32>();
    let mo = s[5..7].parse::<u32>();
    let d = s[8..10].parse::<u32>();
    y.is_ok() && (1..=12).contains(&mo.unwrap_or(0)) && (1..=31).contains(&d.unwrap_or(0))
}

fn v_period(s: &str) -> bool {
    [
        "M1", "M2", "M3", "M5", "M15", "M30", "H1", "H2", "H4", "D1", "W1", "MN1",
    ]
    .contains(&s)
}

fn v_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn v_leverage(s: &str) -> bool {
    match s.strip_prefix("1:") {
        Some(rest) => {
            !rest.is_empty() && rest.len() <= 4 && rest.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn v_currency(s: &str) -> bool {
    !s.is_empty() && s.len() <= 8 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

#[derive(Debug, Default, Deserialize)]
struct RunArgs {
    path: Option<String>,
    symbol: Option<String>,
    period: Option<String>,
    model: Option<u32>,
    from: Option<String>,
    to: Option<String>,
    deposit: Option<f64>,
    currency: Option<String>,
    leverage: Option<String>,
    set: Option<String>,
    timeout_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct OptArgs {
    criterion: Option<u64>,
    mode: Option<u64>,
    max_passes: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DoctorReq {
    check_remote: Option<bool>,
}

fn reject_unknown(args: &Value, allowed: &[&str]) -> Result<(), String> {
    if let Value::Object(map) = args {
        for k in map.keys() {
            if !allowed.contains(&k.as_str()) {
                return Err(format!("unknown parameter '{}'", k));
            }
        }
    }
    Ok(())
}

fn parse_typed<T: DeserializeOwned>(args: &Value, allowed: &[&str]) -> Result<T, ToolError> {
    reject_unknown(args, allowed).map_err(ToolError::InvalidParameters)?;
    serde_json::from_value(args.clone()).map_err(|e| ToolError::InvalidParameters(e.to_string()))
}
fn tail(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut start = s.len() - n;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_string()
}

fn extract_first(hay: &str, prefix: &str) -> Option<String> {
    for line in hay.lines() {
        if let Some(v) = line.trim().strip_prefix(prefix) {
            if !v.is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn extract_last(hay: &str, prefix: &str) -> Option<String> {
    let mut found = None;
    for line in hay.lines() {
        if let Some(v) = line.trim().strip_prefix(prefix) {
            if !v.is_empty() {
                found = Some(v.trim().to_string());
            }
        }
    }
    found
}

fn num_to_str(f: f64) -> String {
    if f.fract() == 0.0 {
        format!("{}", f as i64)
    } else {
        format!("{}", f)
    }
}

fn build_bt_env(
    a: &RunArgs,
    ws: &Path,
    roots: &[PathBuf],
) -> Result<(PathBuf, Vec<(String, String)>, u64), String> {
    let file = resolve_inside(a.path.as_deref().unwrap_or(""), ws, roots, &["mq5", "mqh"])?;
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(v) = &a.symbol {
        if !v_symbol(v) {
            return Err(format!("invalid symbol: {}", v));
        }
        env.push(("MQL_BT_SYMBOL".into(), v.clone()));
    }
    if let Some(v) = &a.period {
        if !v_period(v) {
            return Err(format!("invalid period: {}", v));
        }
        env.push(("MQL_BT_PERIOD".into(), v.clone()));
    }
    if let Some(v) = a.model {
        if !matches!(v, 0 | 1 | 2 | 4) {
            return Err(
                "model must be 0 (every tick), 1 (1-min OHLC), 2 (open prices), or 4 (real ticks)"
                    .into(),
            );
        }
        env.push(("MQL_BT_MODEL".into(), v.to_string()));
    }
    if let Some(v) = &a.from {
        if !v_date(v) {
            return Err(format!("from must be YYYY.MM.DD, got {}", v));
        }
        env.push(("MQL_BT_FROM".into(), v.clone()));
    }
    if let Some(v) = &a.to {
        if !v_date(v) {
            return Err(format!("to must be YYYY.MM.DD, got {}", v));
        }
        env.push(("MQL_BT_TO".into(), v.clone()));
    }
    if let Some(v) = a.deposit {
        if !(0.01..=1_000_000_000_000.0).contains(&v) {
            return Err("deposit out of range".into());
        }
        env.push(("MQL_BT_DEPOSIT".into(), num_to_str(v)));
    }
    if let Some(v) = &a.currency {
        if !v_currency(v) {
            return Err(format!("invalid currency: {}", v));
        }
        env.push(("MQL_BT_CURRENCY".into(), v.clone()));
    }
    if let Some(v) = &a.leverage {
        if !v_leverage(v) {
            return Err(format!("invalid leverage (want 1:N): {}", v));
        }
        env.push(("MQL_BT_LEVERAGE".into(), v.clone()));
    }
    if let Some(v) = &a.set {
        let p = resolve_inside(v, ws, roots, &["set"])?;
        env.push(("MQL_BT_SET".into(), p.display().to_string()));
    }
    let timeout = a.timeout_secs.unwrap_or(1800);
    if !(30..=21_600).contains(&timeout) {
        return Err("timeout_secs must be 30..=21600".into());
    }
    env.push(("MQL_BT_TIMEOUT".into(), timeout.to_string()));
    Ok((file, env, timeout))
}

async fn doctor_value(r: &MqlRouter, check_remote: bool) -> Value {
    let ws = &*r.ws;
    let mut checks: Vec<Value> = Vec::new();
    let add = |name: &str, ok: bool, detail: String, checks: &mut Vec<Value>| {
        checks.push(json!({ "name": name, "ok": ok, "detail": detail }));
    };

    let needed = [
        "compile-mql-remote.sh",
        "tester-mql-remote.sh",
        "optimize-mql-remote.sh",
        "parse-tester-log.py",
        "parse-opt-report.py",
        "check-run-evidence.py",
        "lint-mql.py",
        "gen-set.py",
        "compare-runs.py",
        "check-opt-limits.py",
        "resolve-mql.sh",
    ];
    let missing: Vec<String> = needed
        .iter()
        .filter(|n| !ws.join(SCRIPTS_DIR).join(**n).is_file())
        .map(|n| n.to_string())
        .collect();
    add(
        "scripts",
        missing.is_empty(),
        if missing.is_empty() {
            "all present".into()
        } else {
            missing.join(", ")
        },
        &mut checks,
    );

    let probe = r#"for b in bash python3 ssh scp sha256sum; do if command -v $b >/dev/null 2>&1; then echo have_$b=1; else echo have_$b=0; fi; done"#;
    let mut bin_missing: Vec<String> = Vec::new();
    if let Ok(o) = Command::new("bash").arg("-c").arg(probe).output().await {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            if line.ends_with("=0") {
                if let Some(k) = line.strip_prefix("have_") {
                    bin_missing.push(k.trim_end_matches("=0").to_string());
                }
            }
        }
    }
    add(
        "binaries",
        bin_missing.is_empty(),
        if bin_missing.is_empty() {
            "bash python3 ssh scp sha256sum".into()
        } else {
            format!("missing: {}", bin_missing.join(", "))
        },
        &mut checks,
    );

    let lsp = ws.join("lsp").join("target").join("debug").join("mql-lsp");
    add(
        "lint-server",
        lsp.is_file(),
        lsp.display().to_string(),
        &mut checks,
    );

    add(
        "vm-config",
        r.cfg.vm_mql5_root.is_some(),
        format!(
            "host={} term={} mql5_root={}",
            r.cfg.vm_host,
            r.cfg.vm_terminal,
            r.cfg
                .vm_mql5_root
                .clone()
                .unwrap_or_else(|| "NOT SET".into())
        ),
        &mut checks,
    );

    let jobs_summary = json!({
        "active": r.jobs.active_count(),
        "total": r.jobs.list().len(),
        "reconciled_at_startup": r.jobs.reconciled_at_startup(),
        "store": r.jobs.dir().display().to_string(),
    });

    let lock_list: Vec<Value> = r
        .locks
        .status()
        .into_iter()
        .map(|(w, j, alive)| json!({"worker": w, "job": j, "alive": alive}))
        .collect();

    let mut remote = Value::Null;
    if check_remote {
        let h = r.cfg.vm_host.clone();
        let ssh_args = [
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=6",
            "-o",
            "ControlMaster=auto",
            "-o",
            "ControlPath=/tmp/zedmql-%C",
            "-o",
            "ControlPersist=10m",
        ];
        let echo_res = tokio::time::timeout(
            Duration::from_secs(12),
            Command::new("ssh")
                .args(ssh_args)
                .arg(&h)
                .arg("echo VM_OK")
                .output(),
        )
        .await;
        let vm_ok =
            matches!(&echo_res, Ok(Ok(o)) if String::from_utf8_lossy(&o.stdout).contains("VM_OK"));
        let mut term_count = String::from("unknown");
        if vm_ok {
            let ps = [
                "powershell",
                "-NoProfile",
                "-Command",
                "@(Get-Process terminal64 -ErrorAction SilentlyContinue).Count",
            ];
            if let Ok(Ok(o)) = tokio::time::timeout(
                Duration::from_secs(15),
                Command::new("ssh").args(ssh_args).arg(&h).args(ps).output(),
            )
            .await
            {
                term_count = String::from_utf8_lossy(&o.stdout).trim().to_string();
            }
        }
        remote = json!({ "ssh_ok": vm_ok, "vm_terminal64_processes": term_count });
        add(
            "remote-worker",
            vm_ok,
            format!(
                "host {} reachable: {}; terminal64 running: {}",
                h, vm_ok, term_count
            ),
            &mut checks,
        );
    }

    let local_fail = checks
        .iter()
        .any(|c| c["name"] == json!("scripts") && c["ok"] == json!(false));
    let status = if local_fail {
        "fail"
    } else if checks.iter().any(|c| c["ok"] == json!(false))
        || lock_list.iter().any(|l| l["alive"] == json!(true))
    {
        "warn"
    } else {
        "ok"
    };
    json!({
        "status": status,
        "workspace": ws.display().to_string(),
        "approved_roots": r.roots.iter().map(|p| p.display().to_string()).collect::<Vec<String>>(),
        "checks": checks,
        "jobs": jobs_summary,
        "worker_locks": lock_list,
        "remote": remote,
    })
}

async fn run_worker_script(
    r: &MqlRouter,
    tool: &str,
    script: &str,
    file: &Path,
    env: &[(String, String)],
    timeout: Duration,
) -> Result<Value, String> {
    let worker = format!("vm-{}", r.cfg.vm_host);
    let job = r
        .jobs
        .create(tool, Some(worker.clone()))
        .map_err(|e| e.to_string())?;
    let cp = std::process::id();
    if let Err(busy) = r.locks.acquire(&worker, &job.id, cp) {
        let _ = r.jobs.finish(&job.id, JobState::Failed, Some(busy.clone()));
        return Err(business_note(&busy, &job.id));
    }
    if let Err(e) = r.jobs.update(&job.id, Some(JobState::Queued), |j| {
        j.state = JobState::Preparing
    }) {
        r.locks.release(&worker, &job.id);
        return Err(e.to_string());
    }
    let mut cmd = Command::new("bash");
    cmd.arg(script).arg(file);
    cmd.current_dir(&*r.ws);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    #[cfg(unix)]
    cmd.process_group(0);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            r.locks.release(&worker, &job.id);
            let _ = r.jobs.finish(
                &job.id,
                JobState::Failed,
                Some(format!("spawn failed: {}", e)),
            );
            return Err(format!("spawn failed: {}", e));
        }
    };
    if let Some(pid) = child.id() {
        let _ = r.jobs.update(&job.id, Some(JobState::Preparing), |j| {
            j.pid = Some(pid);
            j.state = JobState::Running;
        });
    }
    let outcome = tokio::time::timeout(timeout, child.wait_with_output()).await;
    r.locks.release(&worker, &job.id);
    match outcome {
        Err(_) => {
            if let Some(pid) = r.jobs.get(&job.id).and_then(|j| j.pid) {
                let _ = Command::new("/bin/kill")
                    .args(["-TERM", &format!("-{}", pid)])
                    .status()
                    .await;
            }
            let _ = r.jobs.finish(
                &job.id,
                JobState::RemoteStateUnknown,
                Some("controller deadline exceeded: the worker script was signaled but the remote MT5 terminal state is NOT confirmed; check mql_doctor before reusing this worker".into()),
            );
            Ok(
                json!({ "job_id": job.id, "state": "RemoteStateUnknown", "note": "deadline exceeded; remote state unconfirmed" }),
            )
        }
        Ok(Err(e)) => {
            let _ = r.jobs.finish(
                &job.id,
                JobState::Failed,
                Some(format!("collect error: {}", e)),
            );
            Err(format!("collect error: {}", e))
        }
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let run_id = extract_first(&stdout, "== Run ID: ");
            let run_dir = extract_last(&stdout, "Saved in ");
            let _ = r.jobs.update(&job.id, None, |j| {
                j.run_id = run_id.clone();
                j.run_dir = run_dir.clone();
            });
            let exit = output.status.code().unwrap_or(-1);
            let state = if exit == 0 {
                JobState::Completed
            } else {
                JobState::Failed
            };
            let result = json!({
                "exit_code": exit,
                "run_id": run_id,
                "run_dir": run_dir,
                "stdout_tail": tail(&stdout, 4000),
                "stderr_tail": tail(&stderr, 2000),
            })
            .to_string();
            let j = r
                .jobs
                .finish(&job.id, state, Some(result))
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "job_id": j.id,
                "state": format!("{:?}", j.state),
                "run_id": run_id,
                "run_dir": run_dir,
                "exit_code": exit,
            }))
        }
    }
}

fn business_note(busy: &str, job_id: &str) -> String {
    format!(
        "{}; accepted job {} was recorded as Failed and NOT launched",
        busy, job_id
    )
}

fn read_run(dir: &Path) -> Option<Value> {
    let manifest = dir.join("manifest.json");
    if !manifest.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&manifest).ok()?;
    let m: Value = serde_json::from_str(&text).ok()?;
    let result_path = dir.join("result.json");
    let mut result_status = Value::Null;
    if result_path.is_file() {
        if let Ok(rt) = std::fs::read_to_string(&result_path) {
            if let Ok(rv) = serde_json::from_str::<Value>(&rt) {
                result_status = rv.get("status").cloned().unwrap_or(Value::Null);
            }
        }
    }
    let get = |a: &str, b: &str| {
        m.get(a)
            .and_then(|x| x.get(b))
            .cloned()
            .unwrap_or(Value::Null)
    };
    Some(json!({
        "run_id": m.get("run_id").cloned().unwrap_or_else(|| Value::String(dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
        "kind": m.get("kind").cloned().unwrap_or(Value::String("backtest".into())),
        "timestamp": m.get("timestamp").cloned().unwrap_or(Value::Null),
        "symbol": get("conditions", "symbol"),
        "period": get("conditions", "period"),
        "from_date": get("conditions", "from_date"),
        "to_date": get("conditions", "to_date"),
        "broker_server": m.get("broker").and_then(|b| b.get("server")).cloned().unwrap_or(Value::Null),
        "data_coverage": m.get("data_coverage").cloned().unwrap_or(Value::Null),
        "has_result": result_path.is_file(),
        "result_status": result_status,
        "dir": dir.display().to_string(),
    }))
}
fn resolve_dir_inside(raw: &str, ws: &Path, roots: &[PathBuf]) -> Result<PathBuf, String> {
    let p = Path::new(raw);
    for c in p.components() {
        if matches!(c, Component::ParentDir) {
            return Err("path traversal ('..') is not allowed".into());
        }
    }
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        ws.join(p)
    };
    let canon = std::fs::canonicalize(&abs).map_err(|_| format!("not found: {}", raw))?;
    if !roots.iter().any(|r| canon.starts_with(r)) {
        return Err(format!(
            "{} is outside approved workspace roots",
            canon.display()
        ));
    }
    if !canon.is_dir() {
        return Err(format!("not a directory: {}", canon.display()));
    }
    Ok(canon)
}

impl MqlRouter {
    pub async fn build() -> Self {
        let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let jobs = Arc::new(
            JobManager::open(&ws.join(".mql").join("jobs"))
                .expect("cannot open job store under .mql/jobs"),
        );
        let locks = Arc::new(
            Locks::open(&ws.join(".mql").join("locks"))
                .expect("cannot open lock store under .mql/locks"),
        );
        let cfg = Arc::new(load_cfg(&ws).await);
        let roots = Arc::new(allowed_roots(&ws));
        Self {
            jobs,
            locks,
            ws: Arc::new(ws),
            cfg,
            roots,
        }
    }

    fn runs_root(&self, workspace: &Option<String>) -> Result<PathBuf, String> {
        let base = match workspace {
            Some(w) => resolve_dir_inside(w, &self.ws, &self.roots)?,
            None => (*self.ws).clone(),
        };
        Ok(base.join(".mql").join("runs"))
    }
}

const RUN_KEYS: &[&str] = &[
    "path",
    "symbol",
    "period",
    "model",
    "from",
    "to",
    "deposit",
    "currency",
    "leverage",
    "set",
    "timeout_secs",
];
const OPT_KEYS: &[&str] = &[
    "path",
    "criterion",
    "mode",
    "max_passes",
    "symbol",
    "period",
    "model",
    "from",
    "to",
    "deposit",
    "currency",
    "leverage",
    "set",
    "timeout_secs",
];

impl Router for MqlRouter {
    fn name(&self) -> String {
        "mql-mcp".into()
    }

    fn instructions(&self) -> String {
        "MCP server for MQL5 development and MT5 Strategy Tester backtesting. All runs are recorded as durable jobs and immutable artifacts under .mql/.".into()
    }

    fn capabilities(&self) -> ServerCapabilities {
        CapabilitiesBuilder::new().with_tools(false).build()
    }

    fn list_tools(&self) -> Vec<Tool> {
        tools::list_tools()
    }

    fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Content>, ToolError>> + Send + 'static>> {
        let name = tool_name.to_string();
        let this = self.clone();

        Box::pin(async move {
            match name.as_str() {
                "mql_doctor" => {
                    let req: DoctorReq = parse_typed(&arguments, &["check_remote"])?;
                    let v = doctor_value(&this, req.check_remote.unwrap_or(true)).await;
                    Ok(vec![Content::text(
                        serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into()),
                    )])
                }
                "mql_lint" => {
                    #[derive(Deserialize, Default)]
                    #[serde(default)]
                    struct LintReq {
                        path: String,
                    }
                    let req: LintReq = parse_typed(&arguments, &["path"])?;
                    let file = resolve_inside(&req.path, &this.ws, &this.roots, &["mq5", "mqh"])
                        .map_err(ToolError::InvalidParameters)?;
                    let out = Command::new("python3")
                        .arg(this.ws.join(SCRIPTS_DIR).join("lint-mql.py"))
                        .arg(&file)
                        .current_dir(&*this.ws)
                        .output()
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    if out.status.success() {
                        Ok(vec![Content::text(stdout)])
                    } else {
                        Err(ToolError::ExecutionError(format!(
                            "lint failed (exit {}): {}{}",
                            out.status.code().unwrap_or(-1),
                            stdout,
                            stderr
                        )))
                    }
                }
                "mql_compile" => {
                    #[derive(Deserialize, Default)]
                    #[serde(default)]
                    struct CompileReq {
                        path: String,
                        syntax_only: bool,
                    }
                    let req: CompileReq = parse_typed(&arguments, &["path", "syntax_only"])?;
                    let file = resolve_inside(&req.path, &this.ws, &this.roots, &["mq5", "mqh"])
                        .map_err(ToolError::InvalidParameters)?;
                    let script = this.ws.join(SCRIPTS_DIR).join("compile-mql-remote.sh");
                    let env: Vec<(String, String)> = Vec::new();
                    let res = run_worker_script(
                        &this,
                        "mql_compile",
                        &script.display().to_string(),
                        &file,
                        &env,
                        Duration::from_secs(600),
                    )
                    .await;
                    match res {
                        Ok(v) => Ok(vec![Content::text(
                            serde_json::to_string_pretty(&v).unwrap_or_default(),
                        )]),
                        Err(e) => Err(ToolError::ExecutionError(e)),
                    }
                }
                "mql_backtest" => {
                    let req: RunArgs = parse_typed(&arguments, RUN_KEYS)?;
                    let (file, env, timeout) = build_bt_env(&req, &this.ws, &this.roots)
                        .map_err(ToolError::InvalidParameters)?;
                    let script = this.ws.join(SCRIPTS_DIR).join("tester-mql-remote.sh");
                    let res = run_worker_script(
                        &this,
                        "mql_backtest",
                        &script.display().to_string(),
                        &file,
                        &env,
                        timeout_to_deadline(timeout),
                    )
                    .await;
                    match res {
                        Ok(v) => Ok(vec![Content::text(
                            serde_json::to_string_pretty(&v).unwrap_or_default(),
                        )]),
                        Err(e) => Err(ToolError::ExecutionError(e)),
                    }
                }
                "mql_optimize" => {
                    let req: RunArgs = parse_typed(&arguments, OPT_KEYS)?;
                    let opt: OptArgs = parse_typed(&arguments, OPT_KEYS)?;
                    let (file, mut env, mut timeout) = build_bt_env(&req, &this.ws, &this.roots)
                        .map_err(ToolError::InvalidParameters)?;
                    if req.timeout_secs.is_none() {
                        timeout = 7200;
                        if let Some(pair) = env.iter_mut().find(|(k, _)| k == "MQL_BT_TIMEOUT") {
                            pair.1 = timeout.to_string();
                        } else {
                            env.push(("MQL_BT_TIMEOUT".into(), timeout.to_string()));
                        }
                    }
                    let criterion = opt.criterion.unwrap_or(0);
                    if criterion > 6 {
                        return Err(ToolError::InvalidParameters(
                            "criterion must be 0..=6".into(),
                        ));
                    }
                    let mode = opt.mode.unwrap_or(1);
                    if !matches!(mode, 1 | 2) {
                        return Err(ToolError::InvalidParameters(
                            "mode must be 1 (slow complete) or 2 (fast genetic)".into(),
                        ));
                    }
                    let max_passes = opt.max_passes.unwrap_or(100_000);
                    if !(1..=1_000_000).contains(&max_passes) {
                        return Err(ToolError::InvalidParameters(
                            "max_passes must be 1..=1000000".into(),
                        ));
                    }
                    env.push(("MQL_OPT_CRITERION".into(), criterion.to_string()));
                    env.push(("MQL_OPT_MODE".into(), mode.to_string()));
                    env.push(("MQL_OPT_MAX_PASSES".into(), max_passes.to_string()));
                    let script = this.ws.join(SCRIPTS_DIR).join("optimize-mql-remote.sh");
                    let res = run_worker_script(
                        &this,
                        "mql_optimize",
                        &script.display().to_string(),
                        &file,
                        &env,
                        timeout_to_deadline(timeout),
                    )
                    .await;
                    match res {
                        Ok(v) => Ok(vec![Content::text(
                            serde_json::to_string_pretty(&v).unwrap_or_default(),
                        )]),
                        Err(e) => Err(ToolError::ExecutionError(e)),
                    }
                }
                "mql_job_status" => {
                    #[derive(Deserialize)]
                    struct JobIdReq {
                        job_id: String,
                    }
                    let req: JobIdReq = parse_typed(&arguments, &["job_id"])?;
                    match this.jobs.get(&req.job_id) {
                        Some(job) => Ok(vec![Content::text(
                            serde_json::to_string_pretty(&job).unwrap_or_default(),
                        )]),
                        None => Err(ToolError::NotFound(format!("job {} not found", req.job_id))),
                    }
                }
                "mql_job_cancel" => {
                    #[derive(Deserialize)]
                    struct JobIdReq2 {
                        job_id: String,
                    }
                    let req: JobIdReq2 = parse_typed(&arguments, &["job_id"])?;
                    let job = this.jobs.get(&req.job_id).ok_or_else(|| {
                        ToolError::NotFound(format!("job {} not found", req.job_id))
                    })?;
                    if job.state.is_terminal() {
                        return Err(ToolError::InvalidParameters(format!(
                            "job already terminal ({:?})",
                            job.state
                        )));
                    }
                    let mut killed = false;
                    if let Some(pid) = job.pid {
                        killed = Command::new("/bin/kill")
                            .args(["-TERM", &format!("-{}", pid)])
                            .status()
                            .await
                            .map(|s| s.success())
                            .unwrap_or(false);
                    }
                    let j = this
                        .jobs
                        .request_cancel(&req.job_id)
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(serde_json::to_string_pretty(&json!({
                        "job_id": j.id,
                        "state": format!("{:?}", j.state),
                        "local_group_signaled": killed,
                        "note": r#"TERM was sent to this job's own process group only; the remote MT5 terminal is left untouched: verify mql_doctor shows no running worker before starting another run"#,
                    }))
                    .unwrap_or_default())])
                }
                "mql_list_runs" => {
                    #[derive(Deserialize, Default)]
                    #[serde(default)]
                    struct ListReq {
                        limit: Option<usize>,
                        workspace: Option<String>,
                    }
                    let req: ListReq = parse_typed(&arguments, &["limit", "workspace"])?;
                    let limit = req.limit.unwrap_or(10).clamp(1, 200);
                    let root = this
                        .runs_root(&req.workspace)
                        .map_err(ToolError::InvalidParameters)?;
                    let mut names: Vec<String> = std::fs::read_dir(&root)
                        .map(|rd| {
                            rd.flatten()
                                .map(|e| e.file_name().to_string_lossy().to_string())
                                .filter(|n| !n.starts_with('.'))
                                .collect()
                        })
                        .unwrap_or_default();
                    names.sort();
                    names.reverse();
                    let runs: Vec<Value> = names
                        .iter()
                        .take(limit)
                        .filter_map(|n| read_run(&root.join(n)))
                        .collect();
                    let out = json!({ "runs_dir": root.display().to_string(), "count": runs.len(), "runs": runs });
                    Ok(vec![Content::text(
                        serde_json::to_string_pretty(&out).unwrap_or_default(),
                    )])
                }
                "mql_get_run" => {
                    #[derive(Deserialize, Default)]
                    #[serde(default)]
                    struct GetRunReq {
                        run_id: String,
                        workspace: Option<String>,
                    }
                    let req: GetRunReq = parse_typed(&arguments, &["run_id", "workspace"])?;
                    validate_run_id(&req.run_id).map_err(ToolError::InvalidParameters)?;
                    let root = this
                        .runs_root(&req.workspace)
                        .map_err(ToolError::InvalidParameters)?;
                    let dir = root.join(&req.run_id);
                    if !dir.is_dir() {
                        return Err(ToolError::NotFound(format!("run {} not found", req.run_id)));
                    }
                    let mut out = Map::new();
                    for key in ["manifest.json", "result.json"] {
                        if let Ok(text) = std::fs::read_to_string(dir.join(key)) {
                            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                                out.insert(key.trim_end_matches(".json").to_string(), v);
                            }
                        }
                    }
                    if let Ok(text) = std::fs::read_to_string(dir.join("summary.txt")) {
                        out.insert("summary_tail".to_string(), Value::String(tail(&text, 4000)));
                    }
                    if out.is_empty() {
                        return Err(ToolError::NotFound(format!(
                            "run {} has no readable artifacts",
                            req.run_id
                        )));
                    }
                    Ok(vec![Content::text(
                        serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default(),
                    )])
                }
                "mql_compare_runs" => {
                    #[derive(Deserialize, Default)]
                    #[serde(default)]
                    struct CompareReq {
                        base_run_id: String,
                        compare_run_id: String,
                        workspace: Option<String>,
                    }
                    let req: CompareReq =
                        parse_typed(&arguments, &["base_run_id", "compare_run_id", "workspace"])?;
                    validate_run_id(&req.base_run_id).map_err(ToolError::InvalidParameters)?;
                    validate_run_id(&req.compare_run_id).map_err(ToolError::InvalidParameters)?;
                    let root = this
                        .runs_root(&req.workspace)
                        .map_err(ToolError::InvalidParameters)?;
                    let a = root.join(&req.base_run_id);
                    let b = root.join(&req.compare_run_id);
                    if !a.is_dir() {
                        return Err(ToolError::NotFound(format!(
                            "run {} not found",
                            req.base_run_id
                        )));
                    }
                    if !b.is_dir() {
                        return Err(ToolError::NotFound(format!(
                            "run {} not found",
                            req.compare_run_id
                        )));
                    }
                    let out = Command::new("python3")
                        .arg(this.ws.join(SCRIPTS_DIR).join("compare-runs.py"))
                        .arg(&a)
                        .arg(&b)
                        .current_dir(&*this.ws)
                        .output()
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    let text = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    if out.status.success() {
                        Ok(vec![Content::text(text)])
                    } else {
                        Err(ToolError::ExecutionError(text))
                    }
                }
                _ => Err(ToolError::NotFound(format!(
                    "Tool {} not implemented",
                    name
                ))),
            }
        })
    }

    fn list_resources(&self) -> Vec<Resource> {
        vec![]
    }

    fn read_resource(
        &self,
        _uri: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ResourceError>> + Send + 'static>> {
        Box::pin(async move { Err(ResourceError::NotFound("Not found".into())) })
    }

    fn list_prompts(&self) -> Vec<Prompt> {
        vec![]
    }

    fn get_prompt(
        &self,
        _prompt_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, PromptError>> + Send + 'static>> {
        Box::pin(async move { Err(PromptError::NotFound("Not found".into())) })
    }
}

fn timeout_to_deadline(timeout_secs: u64) -> Duration {
    Duration::from_secs(timeout_secs + 300)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let router = MqlRouter::build().await;
    tracing::info!("Starting mql-mcp server");
    let rec = router.jobs.reconciled_at_startup();
    if !rec.is_empty() {
        tracing::warn!(
            "reconciled {} interrupted job(s) to RemoteStateUnknown: {:?}",
            rec.len(),
            rec
        );
    }

    let service = RouterService(router);
    let server = Server::new(service);

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let transport = ByteTransport::new(stdin, stdout);

    server.run(transport).await?;
    Ok(())
}
