use std::sync::Arc;
use std::time::Duration;

use super::ExtensionSupervisor;

/// Runtime-owned collection that converges every host surface on one
/// deterministic extension shutdown path.
pub struct ExtensionSupervisorSet {
    supervisors: Vec<Arc<ExtensionSupervisor>>,
    grace: Duration,
}

impl ExtensionSupervisorSet {
    pub fn new(supervisors: Vec<Arc<ExtensionSupervisor>>) -> Self {
        Self {
            supervisors,
            grace: Duration::from_millis(500),
        }
    }

    pub async fn shutdown(&mut self) {
        for supervisor in self.supervisors.drain(..) {
            if let Err(error) = supervisor.shutdown(self.grace).await {
                tracing::warn!(%error, "extension cleanup failed");
            }
        }
    }
}
