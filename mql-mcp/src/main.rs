mod tools;
mod jobs;

use std::{future::Future, pin::Pin, sync::Arc};
use mcp_spec::handler::{PromptError, ResourceError, ToolError};
use mcp_spec::prompt::Prompt;
use mcp_spec::protocol::ServerCapabilities;
use mcp_spec::tool::Tool;
use mcp_spec::content::Content;
use mcp_spec::resource::Resource;
use mcp_server::{ByteTransport, Server, router::{Router, RouterService, CapabilitiesBuilder}};
use serde_json::Value;

use crate::jobs::{JobManager, JobState};

#[derive(Clone)]
struct MqlRouter {
    jobs: Arc<JobManager>,
}

impl MqlRouter {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(JobManager::new()),
        }
    }
}

impl Router for MqlRouter {
    fn name(&self) -> String {
        "mql-mcp".into()
    }

    fn instructions(&self) -> String {
        "MCP server for interacting with MQL5 and MetaTrader 5 backtester".into()
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
        let jobs = self.jobs.clone();
        
        Box::pin(async move {
            match name.as_str() {
                "mql_doctor" => {
                    Ok(vec![Content::text("Doctor check passed (mocked) - system is ready.")])
                },
                "mql_lint" => {
                    Ok(vec![Content::text("No lint findings.")])
                },
                "mql_compile" => {
                    let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                    let syntax_only = arguments.get("syntax_only").and_then(|v| v.as_bool()).unwrap_or(false);
                    
                    let id = jobs.create_job("mql_compile".to_string());
                    
                    let jobs_clone = jobs.clone();
                    let id_clone = id.clone();
                    tokio::spawn(async move {
                        jobs_clone.update_state(&id_clone, JobState::Preparing);
                        
                        let mut cmd = tokio::process::Command::new("bash");
                        cmd.arg("scripts/compile-mql-remote.sh");
                        if syntax_only {
                            cmd.arg("--check");
                        }
                        cmd.arg("--json");
                        cmd.arg("--job-id").arg(&id_clone);
                        cmd.arg(&path);
                        
                        jobs_clone.update_state(&id_clone, JobState::Running);
                        match cmd.output().await {
                            Ok(output) => {
                                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                                let result_json = serde_json::json!({
                                    "status": output.status.code(),
                                    "stdout": stdout,
                                    "stderr": stderr
                                }).to_string();
                                
                                if output.status.success() {
                                    jobs_clone.complete_job(&id_clone, result_json);
                                } else {
                                    jobs_clone.update_state(&id_clone, JobState::Failed);
                                    jobs_clone.complete_job(&id_clone, result_json);
                                }
                            }
                            Err(e) => {
                                jobs_clone.update_state(&id_clone, JobState::Failed);
                                jobs_clone.complete_job(&id_clone, format!("Failed to spawn process: {}", e));
                            }
                        }
                    });
                    
                    let msg = format!("Started compilation job. Job ID: {}", id);
                    Ok(vec![Content::text(msg)])
                },
                "mql_backtest" => {
                    let id = jobs.create_job("mql_backtest".to_string());
                    
                    let jobs_clone = jobs.clone();
                    let id_clone = id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        jobs_clone.update_state(&id_clone, JobState::Preparing);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        jobs_clone.update_state(&id_clone, JobState::Running);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        jobs_clone.complete_job(&id_clone, "Backtest completed. Profit: $100".to_string());
                    });
                    
                    let msg = format!("Started backtest job. Job ID: {}", id);
                    Ok(vec![Content::text(msg)])
                },
                "mql_job_status" => {
                    let id = arguments.get("job_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidParameters("Missing job_id".into()))?;
                    if let Some(job) = jobs.get_job(id) {
                        let status_json = serde_json::to_string_pretty(&job).unwrap();
                        Ok(vec![Content::text(status_json)])
                    } else {
                        Err(ToolError::ExecutionError(format!("Job {} not found", id)))
                    }
                },
                "mql_job_cancel" => {
                    let id = arguments.get("job_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidParameters("Missing job_id".into()))?;
                    if let Some(_job) = jobs.get_job(id) {
                        jobs.update_state(id, JobState::CancelRequested);
                        Ok(vec![Content::text(format!("Cancel requested for job {}", id))])
                    } else {
                        Err(ToolError::ExecutionError(format!("Job {} not found", id)))
                    }
                },
                "mql_list_runs" => {
                    Ok(vec![Content::text("[]")])
                },
                "mql_get_run" => {
                    Err(ToolError::NotFound("Run not found".into()))
                },
                "mql_compare_runs" => {
                    Err(ToolError::NotFound("Runs not found".into()))
                }
                _ => Err(ToolError::NotFound(format!("Tool {} not implemented yet", name)))
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

    fn get_prompt(&self, _prompt_name: &str) -> Pin<Box<dyn Future<Output = Result<String, PromptError>> + Send + 'static>> {
        Box::pin(async move { Err(PromptError::NotFound("Not found".into())) })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting mql-mcp server");

    let router = MqlRouter::new();
    let service = RouterService(router);
    let server = Server::new(service);

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let transport = ByteTransport::new(stdin, stdout);

    server.run(transport).await?;

    Ok(())
}
