// cli.rs — argument definitions
// nothing fancy, clap derive does the heavy lifting

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name    = "phantom",
    about   = "Phantom-C2 teamserver",
    version = "0.1.0",
    author  = "Light <neok1ra@proton.me>",
    long_about = None,
)]
pub struct Cli {
    /// address:port for the implant TLS listener
    #[arg(long, default_value = "0.0.0.0:4444")]
    pub bind: String,

    /// address:port for the REST + WebSocket API (operator/UI)
    #[arg(long, default_value = "127.0.0.1:8443")]
    pub api_bind: String,

    /// TLS cert (PEM) for the implant listener
    #[arg(long, default_value = "certs/server.crt")]
    pub cert: String,

    /// TLS key (PEM)
    #[arg(long, default_value = "certs/server.key")]
    pub key: String,

    /// CA cert for mTLS implant verification (optional but recommended)
    #[arg(long)]
    pub ca_cert: Option<String>,

    /// debug logging
    #[arg(short, long)]
    pub verbose: bool,

    /// skip TLS — plaintext TCP. lab/ctf only, will warn loudly
    #[arg(long)]
    pub no_tls: bool,

    // this is the flag i actually use so i don't have to type --no-tls
    // (tls=true means TLS is ON, which is what you want by default)
    #[arg(skip)]
    pub tls: bool,
}

impl Cli {
    // called after parse() to set the derived tls flag
    // clap can't express "default true" cleanly with a bool flag so this is easier
    pub fn finalize(mut self) -> Self {
        self.tls = !self.no_tls;
        self
    }
}
// cli args
