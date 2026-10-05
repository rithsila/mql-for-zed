use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub tool: String,
    pub state: JobState,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub result: Option<String>,
}

pub struct JobManager {
    jobs: Mutex<HashMap<String, Job>>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
        }
    }

    pub fn create_job(&self, tool: String) -> String {
        let id = Uuid::new_v4().to_string();
        let job = Job {
            id: id.clone(),
            tool,
            state: JobState::Queued,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            result: None,
        };
        self.jobs.lock().unwrap().insert(id.clone(), job);
        id
    }

    pub fn get_job(&self, id: &str) -> Option<Job> {
        self.jobs.lock().unwrap().get(id).cloned()
    }

    pub fn update_state(&self, id: &str, state: JobState) {
        if let Some(job) = self.jobs.lock().unwrap().get_mut(id) {
            job.state = state;
            job.updated_at = SystemTime::now();
        }
    }
    
    pub fn complete_job(&self, id: &str, result: String) {
        if let Some(job) = self.jobs.lock().unwrap().get_mut(id) {
            job.state = JobState::Completed;
            job.result = Some(result);
            job.updated_at = SystemTime::now();
        }
    }
}
