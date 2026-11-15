//! Desktop Shell State
//!
//! Holds only the thin IPC client handle to the `benchhubd` background daemon.

use crate::daemon_client::DaemonClient;
use std::sync::Arc;

pub struct AppState {
    pub daemon: Arc<DaemonClient>,
}

impl AppState {
    pub fn new(daemon: Arc<DaemonClient>) -> Self {
        Self { daemon }
    }
}
