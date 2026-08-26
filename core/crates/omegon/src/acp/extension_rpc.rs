//! ACP extension route inventory.
//!
//! Execution is worker-owned and enters the EventBus admission path. This
//! erased handle only records which live extensions published an ACP route.

use std::sync::Arc;

pub(super) trait ExtensionRpcHandleIdentity: Send + Sync {
    fn extension_name(&self) -> &str;
}

impl ExtensionRpcHandleIdentity for crate::extensions::ExtensionPollingHandle {
    fn extension_name(&self) -> &str {
        self.extension_name()
    }
}

pub(super) type ExtensionRpcHandle = Arc<dyn ExtensionRpcHandleIdentity>;

pub(super) fn erase_extension_rpc_handles(
    handles: std::collections::BTreeMap<String, crate::extensions::ExtensionPollingHandle>,
) -> std::collections::BTreeMap<String, ExtensionRpcHandle> {
    handles
        .into_iter()
        .map(|(name, handle)| (name, Arc::new(handle) as ExtensionRpcHandle))
        .collect()
}
