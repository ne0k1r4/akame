// api.rs — REST + WebSocket operator API
//
// routes:
//   GET  /sessions            — list all sessions
//   GET  /sessions/:id        — get one session
//   POST /sessions/:id/task   — queue a task
//   GET  /sessions/:id/results — get all results for session
//   GET  /tasks/:id/result    — get specific result
//   GET  /ws                  — WebSocket for live session events (TODO: push on new checkin)
//
// auth: none yet. bind to 127.0.0.1 and add a token header when you expose this.
// i know. it's on the TODO list.

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::{
    error::PhantomError,
    session::SessionStore,
    task::{Task, TaskKind, TaskQueue},
};

#[derive(Clone)]
struct AppState {
    sessions: SessionStore,
    tasks:    TaskQueue,
}

pub async fn run(bind: String, sessions: SessionStore, tasks: TaskQueue) -> anyhow::Result<()> {
    let state = AppState { sessions, tasks };

    let app = Router::new()
        .route("/sessions",              get(list_sessions))
        .route("/sessions/:id",          get(get_session))
        .route("/sessions/:id/task",     post(queue_task))
        .route("/sessions/:id/results",  get(session_results))
        .route("/tasks/:id/result",      get(task_result))
        .route("/ws",                    get(ws_handler))
        .layer(CorsLayer::permissive())  // fine, it's operator-local
        .with_state(state);

    let addr: SocketAddr = bind.parse()?;
    info!(addr = %addr, "operator API ready");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── handlers ──────────────────────────────────────────────────────────────────

async fn list_sessions(
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    let sessions: Vec<_> = s.sessions.iter().map(|e| e.value().clone()).collect();
    Json(serde_json::json!({ "sessions": sessions, "count": sessions.len() }))
}

async fn get_session(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, PhantomError> {
    let sess = s.sessions.get(&id)
        .map(|e| e.value().clone())
        .ok_or_else(|| PhantomError::SessionNotFound(id))?;
    Ok(Json(serde_json::json!(sess)))
}

#[derive(Deserialize)]
struct TaskRequest {
    #[serde(flatten)]
    kind: TaskKind,
}

async fn queue_task(
    State(s): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<TaskRequest>,
) -> Result<Json<serde_json::Value>, PhantomError> {
    // make sure session exists before queuing
    if !s.sessions.contains_key(&session_id) {
        return Err(PhantomError::SessionNotFound(session_id));
    }

    let task = Task::new(&session_id, req.kind);
    let task_id = s.tasks.push(task).await
        .map_err(|e| PhantomError::QueueFull(e))?;

    Ok(Json(serde_json::json!({ "task_id": task_id, "queued": true })))
}

async fn session_results(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, PhantomError> {
    if !s.sessions.contains_key(&id) {
        return Err(PhantomError::SessionNotFound(id));
    }
    let results = s.tasks.all_results_for(&id);
    Ok(Json(serde_json::json!({ "results": results })))
}

async fn task_result(
    State(s): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, PhantomError> {
    s.tasks.get_result(&task_id)
        .map(|r| Json(serde_json::json!(r)))
        .ok_or_else(|| PhantomError::SessionNotFound(task_id))
}

// WebSocket handler — currently just echoes session list on connect
// real-time push on new checkin is the TODO here
async fn ws_handler(
    State(s): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, s))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    // send current session snapshot immediately
    let sessions: Vec<_> = state.sessions.iter().map(|e| e.value().clone()).collect();
    let msg = serde_json::json!({ "event": "snapshot", "sessions": sessions });
    let _ = socket.send(Message::Text(msg.to_string())).await;

    while let Some(msg) = tokio::select! {
        res = socket.recv() => {
            match res {
                Some(Ok(msg)) => Some(msg),
                _ => None,
            }
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => None,
    } {
        match msg {
            Message::Ping(b) => { let _ = socket.send(Message::Pong(b)).await; }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
// REST API
// WebSocket handler implementation
