// session.rs — implant session registry
//
// DashMap because multiple tokio tasks touch this concurrently.
// i tried RwLock<HashMap> first and got a deadlock in the API handler
// within about 20 minutes. dashmap just works.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// everything we know about a connected implant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id:         String,
    pub hostname:   String,
    pub username:   String,
    pub os:         String,
    pub arch:       String,
    pub remote_ip:  String,
    pub pid:        u32,
    pub sleep_ms:   u64,   // current beacon interval
    pub jitter_pct: u8,    // % jitter on sleep
    pub checkin_at: DateTime<Utc>,
    pub first_seen: DateTime<Utc>,
    pub alive:      bool,
}

impl Session {
    pub fn new(beacon: &BeaconHello, remote_ip: String) -> Self {
        let now = Utc::now();
        Session {
            id:         Uuid::new_v4().to_string(),
            hostname:   beacon.hostname.clone(),
            username:   beacon.username.clone(),
            os:         beacon.os.clone(),
            arch:       beacon.arch.clone(),
            remote_ip,
            pid:        beacon.pid,
            sleep_ms:   beacon.sleep_ms,
            jitter_pct: beacon.jitter_pct,
            checkin_at: now,
            first_seen: now,
            alive:      true,
        }
    }

    /// update heartbeat fields on re-checkin
    pub fn touch(&mut self) {
        self.checkin_at = Utc::now();
        self.alive      = true;
    }

    /// seconds since last checkin — useful for the "dead?" heuristic
    pub fn idle_secs(&self) -> i64 {
        (Utc::now() - self.checkin_at).num_seconds()
    }
}

/// first message an implant sends after connecting
#[derive(Debug, Deserialize)]
pub struct BeaconHello {
    pub hostname:   String,
    pub username:   String,
    pub os:         String,
    pub arch:       String,
    pub pid:        u32,
    pub sleep_ms:   u64,
    pub jitter_pct: u8,
}

// Arc<DashMap<...>> so the store is cheaply cloneable across tasks
pub type SessionStore = Arc<DashMap<String, Session>>;

pub fn new_store() -> SessionStore {
    Arc::new(DashMap::new())
}

/// thin newtype wrapper so i can add methods without orphan-rule fights
pub struct Sessions(pub SessionStore);

impl Sessions {
    pub fn register(&self, beacon: BeaconHello, remote_ip: String) -> Session {
        let sess = Session::new(&beacon, remote_ip);
        self.0.insert(sess.id.clone(), sess.clone());
        sess
    }

    pub fn touch(&self, id: &str) -> bool {
        if let Some(mut s) = self.0.get_mut(id) {
            s.touch();
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> Vec<Session> {
        self.0.iter().map(|e| e.value().clone()).collect()
    }

    pub fn get(&self, id: &str) -> Option<Session> {
        self.0.get(id).map(|e| e.value().clone())
    }

    /// mark sessions that haven't checked in as dead
    /// called from a background task every ~60s
    pub fn reap_dead(&self, threshold_secs: i64) {
        for mut entry in self.0.iter_mut() {
            if entry.alive && entry.idle_secs() > threshold_secs {
                tracing::warn!(id = %entry.id, "session presumed dead (no checkin for {}s)", threshold_secs);
                entry.alive = false;
            }
        }
    }
}
// session registry
// session lifecycle handling
