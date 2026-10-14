use mcp_spec::tool::Tool;
use serde_json::json;

fn bt_props(
    extra: &mut Vec<(&str, serde_json::Value)>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut props = serde_json::Map::new();
    for (k, v) in extra.drain(..) {
        props.insert(k.to_string(), v);
    }
    props.insert("path".into(), json!({ "type": "string", "description": "Path to the EA source .mq5 (or .mqh with marker)" }));
    props.insert(
        "symbol".into(),
        json!({ "type": "string", "description": "Broker symbol, e.g. XAUUSDc" }),
    );
    props.insert("period".into(), json!({ "type": "string", "enum": ["M1","M2","M3","M5","M15","M30","H1","H2","H4","D1","W1","MN1"] }));
    props.insert("model".into(), json!({ "type": "integer", "enum": [0,1,2,4], "description": "0 every tick, 1 1-min OHLC, 2 open prices, 4 real ticks" }));
    props.insert("from".into(), json!({ "type": "string", "description": "Start date YYYY.MM.DD (required: recent ranges may have no cent-symbol data)" }));
    props.insert(
        "to".into(),
        json!({ "type": "string", "description": "End date YYYY.MM.DD" }),
    );
    props.insert("deposit".into(), json!({ "type": "number" }));
    props.insert(
        "currency".into(),
        json!({ "type": "string", "description": "Account currency, e.g. USC for cent accounts" }),
    );
    props.insert(
        "leverage".into(),
        json!({ "type": "string", "description": "e.g. 1:500" }),
    );
    props.insert(
        "set".into(),
        json!({ "type": "string", "description": "Explicit .set file with EA inputs" }),
    );
    props.insert(
        "timeout_secs".into(),
        json!({ "type": "integer", "description": "Wall-clock budget for the tester run" }),
    );
    props
}

pub fn list_tools() -> Vec<Tool> {
    let doctor_props = json!({ "check_remote": { "type": "boolean", "description": "Probe the broker VM over SSH (default true)" } });
    vec![
        Tool {
            name: "mql_doctor".into(),
            description: "Real readiness check: scripts, binaries, lint server, VM config, jobs/locks state, and optional SSH probe of the worker VM. Returns JSON. Call before starting campaigns.".into(),
            input_schema: json!({ "type": "object", "properties": doctor_props }),
        },
        Tool {
            name: "mql_lint".into(),
            description: "Run the local zed-mql lint rules (same engine as the Zed Problems panel) on one file. Returns JSON diagnostics.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Path to the MQL5 file" } },
                "required": ["path"],
            }),
        },
        Tool {
            name: "mql_compile".into(),
            description: "Compile (or syntax-check) an EA on the worker VM. Job-based: returns a job id; poll mql_job_status.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "syntax_only": { "type": "boolean", "default": false },
                },
                "required": ["path"],
            }),
        },
        Tool {
            name: "mql_backtest".into(),
            description: "Run a backtest on the worker VM: compile, deploy, tester run, artifact capture, and NO-DATA evidence check. Fails when the requested range has no bars. Returns job id and run id/dir.".into(),
            input_schema: json!({ "type": "object", "properties": bt_props(&mut vec![]), "required": ["path"] }),
        },
        Tool {
            name: "mql_optimize".into(),
            description: "Run a bounded optimization: same evidence checks as backtest plus report freshness and pass-count validation via result.json.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "criterion": { "type": "integer", "minimum": 0, "maximum": 6, "default": 0 },
                    "mode": { "type": "integer", "enum": [1, 2], "default": 1 },
                    "max_passes": { "type": "integer", "default": 100000, "maximum": 1000000 },
                },
                "required": ["path"],
            }),
        },
        Tool {
            name: "mql_job_status".into(),
            description: "Durable job state, run id/dir, pid, and stored result. Jobs persist across restarts.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "job_id": { "type": "string" } },
                "required": ["job_id"],
            }),
        },
        Tool {
            name: "mql_job_cancel".into(),
            description: "Cancel a non-terminal job: sends TERM to its own process group only. Never touches another terminal.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "job_id": { "type": "string" } },
                "required": ["job_id"],
            }),
        },
        Tool {
            name: "mql_list_runs".into(),
            description: "List immutable run artifacts (.mql/runs) newest first, with data coverage and result status.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "default": 10 },
                    "workspace": { "type": "string", "description": "Project holding .mql/runs (default: server workspace)" }
                },
            }),
        },
        Tool {
            name: "mql_get_run".into(),
            description: "Full structured result of one run: manifest (incl. data coverage + broker), result.json, summary tail.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "workspace": { "type": "string" }
                },
                "required": ["run_id"],
            }),
        },
        Tool {
            name: "mql_compare_runs".into(),
            description: "Compare two runs (inputs + metrics + material-condition warnings) using the existing compare-runs tool.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "base_run_id": { "type": "string" },
                    "compare_run_id": { "type": "string" },
                    "workspace": { "type": "string" }
                },
                "required": ["base_run_id", "compare_run_id"],
            }),
        },
    ]
}
