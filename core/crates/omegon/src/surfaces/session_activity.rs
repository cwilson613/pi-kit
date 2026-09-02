//! Versioned, renderer-neutral session activity and reconciliation policy.

use serde::{Deserialize, Serialize};

use super::actions::CanonicalAction;

pub(crate) const SESSION_ACTIVITY_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActivityTransport {
    Tui,
    Acp,
    Web,
    Ipc,
    Cli,
    Daemon,
}

impl ActivityTransport {
    pub(crate) const ALL: [Self; 6] = [
        Self::Tui,
        Self::Acp,
        Self::Web,
        Self::Ipc,
        Self::Cli,
        Self::Daemon,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SessionActivityLineageV1 {
    pub(crate) session_id: String,
    pub(crate) stream_id: String,
    pub(crate) runtime_generation: String,
    pub(crate) composition_generation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QueuedActivityV1 {
    pub(crate) prompt_id: String,
    pub(crate) submission_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActiveTurnActivityV1 {
    pub(crate) turn_id: String,
    pub(crate) prompt_id: String,
    pub(crate) phase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TerminalTurnActivityV1 {
    pub(crate) turn_id: String,
    pub(crate) outcome: String,
    pub(crate) reason_code: String,
    pub(crate) authority_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleHealthV1 {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActionDescriptorV1 {
    pub(crate) action: CanonicalAction,
    pub(crate) available: bool,
    pub(crate) owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) denial_reason: Option<String>,
    pub(crate) supported_transports: Vec<ActivityTransport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SessionActivityProjectionV1 {
    pub(crate) schema_version: u16,
    pub(crate) lineage: SessionActivityLineageV1,
    /// The durable authority sequence/frontier. This is not a surface-local counter.
    pub(crate) activity_revision: u64,
    pub(crate) queue: Vec<QueuedActivityV1>,
    pub(crate) active_turn: Option<ActiveTurnActivityV1>,
    pub(crate) terminal_turn: Option<TerminalTurnActivityV1>,
    pub(crate) lifecycle_health: LifecycleHealthV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lifecycle_detail: Option<String>,
    pub(crate) actions: Vec<ActionDescriptorV1>,
}

impl SessionActivityProjectionV1 {
    pub(crate) fn canonical_actions(
        active: bool,
        health: LifecycleHealthV1,
    ) -> Vec<ActionDescriptorV1> {
        let owner = "session-supervisor".to_string();
        let healthy = health != LifecycleHealthV1::Unavailable;
        vec![
            ActionDescriptorV1 {
                action: CanonicalAction::StatusView,
                available: true,
                owner: owner.clone(),
                denial_reason: None,
                supported_transports: ActivityTransport::ALL.to_vec(),
            },
            ActionDescriptorV1 {
                action: CanonicalAction::PromptSubmit,
                available: healthy,
                owner: owner.clone(),
                denial_reason: (!healthy).then(|| "session_authority_unavailable".into()),
                supported_transports: ActivityTransport::ALL.to_vec(),
            },
            ActionDescriptorV1 {
                action: CanonicalAction::TurnCancel,
                available: active && healthy,
                owner: owner.clone(),
                denial_reason: if !healthy {
                    Some("session_authority_unavailable".into())
                } else if !active {
                    Some("no_active_turn".into())
                } else {
                    None
                },
                supported_transports: ActivityTransport::ALL.to_vec(),
            },
            ActionDescriptorV1 {
                action: CanonicalAction::SessionNew,
                available: !active && healthy,
                owner,
                denial_reason: if !healthy {
                    Some("session_authority_unavailable".into())
                } else if active {
                    Some("active_turn_owned_by_supervisor".into())
                } else {
                    None
                },
                supported_transports: ActivityTransport::ALL.to_vec(),
            },
        ]
    }

    pub(crate) fn for_transport(
        &self,
        transport: ActivityTransport,
    ) -> TransportActivityProjectionV1 {
        let actions = self
            .actions
            .iter()
            .filter(|action| action.supported_transports.contains(&transport))
            .cloned()
            .collect();
        TransportActivityProjectionV1 {
            activity: Self {
                actions,
                ..self.clone()
            },
            transport,
            persistent_busy_reconciliation: transport != ActivityTransport::Cli,
            narrowing: if transport == ActivityTransport::Cli {
                vec![TransportNarrowingV1 {
                    field: "persistent_busy_reconciliation".into(),
                    reason: "one_shot_transport".into(),
                }]
            } else {
                Vec::new()
            },
        }
    }

    pub(crate) fn is_durably_closed(&self) -> bool {
        self.active_turn.is_none() && self.terminal_turn.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransportNarrowingV1 {
    pub(crate) field: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransportActivityProjectionV1 {
    pub(crate) activity: SessionActivityProjectionV1,
    pub(crate) transport: ActivityTransport,
    pub(crate) persistent_busy_reconciliation: bool,
    pub(crate) narrowing: Vec<TransportNarrowingV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReconcileDisposition {
    Applied,
    Idempotent,
    IgnoredStale,
    IgnoredUnversioned,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ActivityReconcileError {
    #[error("session activity lineage changed without explicit replacement")]
    CrossLineage,
    #[error("duplicate session activity revision has conflicting semantics")]
    ConflictingDuplicate,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionActivityCache {
    current: Option<SessionActivityProjectionV1>,
}

impl SessionActivityCache {
    pub(crate) fn current(&self) -> Option<&SessionActivityProjectionV1> {
        self.current.as_ref()
    }

    pub(crate) fn replace(&mut self, activity: SessionActivityProjectionV1) {
        self.current = Some(activity);
    }

    pub(crate) fn reconcile(
        &mut self,
        incoming: SessionActivityProjectionV1,
    ) -> Result<ReconcileDisposition, ActivityReconcileError> {
        let Some(current) = self.current.as_ref() else {
            self.current = Some(incoming);
            return Ok(ReconcileDisposition::Applied);
        };
        if current.lineage != incoming.lineage {
            return Err(ActivityReconcileError::CrossLineage);
        }
        match incoming.activity_revision.cmp(&current.activity_revision) {
            std::cmp::Ordering::Greater => {
                self.current = Some(incoming);
                Ok(ReconcileDisposition::Applied)
            }
            std::cmp::Ordering::Less => Ok(ReconcileDisposition::IgnoredStale),
            std::cmp::Ordering::Equal if current == &incoming => {
                Ok(ReconcileDisposition::Idempotent)
            }
            std::cmp::Ordering::Equal => Err(ActivityReconcileError::ConflictingDuplicate),
        }
    }

    pub(crate) fn reconcile_unversioned_active(&self) -> ReconcileDisposition {
        ReconcileDisposition::IgnoredUnversioned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(revision: u64, active: bool) -> SessionActivityProjectionV1 {
        let terminal_turn = (!active).then_some(TerminalTurnActivityV1 {
            turn_id: "turn-1".into(),
            outcome: "completed".into(),
            reason_code: "done".into(),
            authority_sequence: revision,
        });
        SessionActivityProjectionV1 {
            schema_version: SESSION_ACTIVITY_SCHEMA_VERSION,
            lineage: SessionActivityLineageV1 {
                session_id: "session-1".into(),
                stream_id: "stream-1".into(),
                runtime_generation: "runtime-1".into(),
                composition_generation: "composition-1".into(),
            },
            activity_revision: revision,
            queue: Vec::new(),
            active_turn: active.then_some(ActiveTurnActivityV1 {
                turn_id: "turn-1".into(),
                prompt_id: "prompt-1".into(),
                phase: "running".into(),
            }),
            terminal_turn,
            lifecycle_health: LifecycleHealthV1::Healthy,
            lifecycle_detail: None,
            actions: SessionActivityProjectionV1::canonical_actions(
                active,
                LifecycleHealthV1::Healthy,
            ),
        }
    }

    #[test]
    fn reconciliation_is_revision_and_lineage_safe() {
        let mut cache = SessionActivityCache::default();
        assert_eq!(
            cache.reconcile(activity(8, false)).unwrap(),
            ReconcileDisposition::Applied
        );
        assert_eq!(
            cache.reconcile(activity(7, true)).unwrap(),
            ReconcileDisposition::IgnoredStale
        );
        assert!(cache.current().unwrap().is_durably_closed());
        assert_eq!(
            cache.reconcile(activity(8, false)).unwrap(),
            ReconcileDisposition::Idempotent
        );
        assert_eq!(
            cache.reconcile_unversioned_active(),
            ReconcileDisposition::IgnoredUnversioned
        );

        let mut conflict = activity(8, false);
        conflict.lifecycle_health = LifecycleHealthV1::Degraded;
        assert_eq!(
            cache.reconcile(conflict),
            Err(ActivityReconcileError::ConflictingDuplicate)
        );

        let mut replacement = activity(9, true);
        replacement.lineage.stream_id = "stream-2".into();
        assert_eq!(
            cache.reconcile(replacement.clone()),
            Err(ActivityReconcileError::CrossLineage)
        );
        cache.replace(replacement.clone());
        assert_eq!(cache.current(), Some(&replacement));
    }

    #[test]
    fn cli_declares_only_persistent_reconciliation_narrowing() {
        let projection = activity(4, true);
        for transport in ActivityTransport::ALL {
            let adapted = projection.for_transport(transport);
            assert_eq!(adapted.activity.lineage, projection.lineage);
            assert_eq!(
                adapted.activity.activity_revision,
                projection.activity_revision
            );
            assert_eq!(adapted.activity.actions, projection.actions);
            assert_eq!(
                adapted.persistent_busy_reconciliation,
                transport != ActivityTransport::Cli
            );
            assert_eq!(
                adapted.narrowing.is_empty(),
                transport != ActivityTransport::Cli
            );
        }
    }
}
