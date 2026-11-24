pub mod drivers;
pub mod infer;
pub mod plugins;
pub mod providers;
mod service;
pub mod store;
pub mod watcher;

pub use service::run_daemon;

use rw_model::{Config, Event, SourceStatus};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{Mutex, RwLock, broadcast};

/// Live state of one daemon. Config is written only through `Store`.
pub struct AppState {
    pub store: store::Store,
    pub config: RwLock<Config>,
    pub statuses: RwLock<BTreeMap<String, SourceStatus>>,
    pub events: broadcast::Sender<Event>,
    pub token: String,
    pub dist: PathBuf,
    /// One guard per source, shared by the timer and API commands.
    pub running: Arc<Mutex<HashSet<String>>>,
    /// Number of connected UIs; used by WhileWindowOpen mode.
    pub ui_clients: AtomicUsize,
}

impl AppState {
    pub fn ui_connected(&self) -> bool {
        self.ui_clients.load(Ordering::SeqCst) > 0
    }
}

impl AppState {
    pub fn emit(&self, event: Event) {
        // A closed receiver is normal when no UI is attached.
        let _ = self.events.send(event);
    }
}

pub type SharedState = Arc<AppState>;

pub fn default_config_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("rw").join("config.yaml")
}

pub fn default_dist_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("RW_UI_DIST") {
        return PathBuf::from(dir);
    }
    // A launcher may start the shell from any directory. Look for the UI
    // beside the binary (cargo puts it two directories below the repo root).
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            let dist = ancestor.join("ui").join("dist");
            if dist.join("index.html").is_file() {
                return dist;
            }
        }
    }
    PathBuf::from("ui/dist")
}
