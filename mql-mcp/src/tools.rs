use mcp_spec::tool::Tool;
use serde_json::json;

pub fn list_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "mql_doctor".into(),
            description: "Check tools, configured host, and tester readiness.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "mql_lint".into(),
            description: "Return local lint findings.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the MQL5 file"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "mql_compile".into(),
            description: "Start syntax-check or compile job.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the MQL5 file"
                    },
                    "syntax_only": {
                        "type": "boolean",
                        "description": "If true, only check syntax without producing an executable",
                        "default": false
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "mql_backtest".into(),
            description: "Start an explicitly configured backtest.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "expert_path": {
                        "type": "string",
                        "description": "Path to the compiled EA (.ex5)"
                    },
                    "config_path": {
                        "type": "string",
                        "description": "Path to backtest configuration (.set or .ini)"
                    }
                },
                "required": ["expert_path", "config_path"]
            }),
        },
        Tool {
            name: "mql_optimize".into(),
            description: "Start a bounded optimization search for an EA.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the MQL5 file (.mq5)"
                    },
                    "criterion": {
                        "type": "integer",
                        "description": "0=Balance, 1=Profit Factor, 2=Expected Payoff, 3=Drawdown min, 4=Recovery Factor, 5=Sharpe, 6=Custom",
                        "default": 0
                    },
                    "mode": {
                        "type": "integer",
                        "description": "1=Slow complete, 2=Fast genetic",
                        "default": 1
                    },
                    "max_passes": {
                        "type": "integer",
                        "description": "Maximum allowed passes (fail fast if exceeded)",
                        "default": 100000
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "mql_job_status".into(),
            description: "Return job state, stage, and available progress.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "job_id": {
                        "type": "string",
                        "description": "The ID of the job"
                    }
                },
                "required": ["job_id"]
            }),
        },
        Tool {
            name: "mql_job_cancel".into(),
            description: "Request cancellation of an owned job.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "job_id": {
                        "type": "string",
                        "description": "The ID of the job to cancel"
                    }
                },
                "required": ["job_id"]
            }),
        },
        Tool {
            name: "mql_list_runs".into(),
            description: "List saved runs and key metadata.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of runs to return",
                        "default": 10
                    }
                }
            }),
        },
        Tool {
            name: "mql_get_run".into(),
            description: "Return a structured result of a specific run.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "run_id": {
                        "type": "string",
                        "description": "The ID of the run"
                    }
                },
                "required": ["run_id"]
            }),
        },
        Tool {
            name: "mql_compare_runs".into(),
            description: "Compare selected runs.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "base_run_id": {
                        "type": "string",
                        "description": "The baseline run ID"
                    },
                    "compare_run_id": {
                        "type": "string",
                        "description": "The run ID to compare against baseline"
                    }
                },
                "required": ["base_run_id", "compare_run_id"]
            }),
        },
    ]
}
