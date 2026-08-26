//! Project DashboardHandles + HarnessStatus into IpcStateSnapshot.

use omegon_traits::{
    IpcChangeSnapshot, IpcChildSnapshot, IpcCleaveSnapshot, IpcDesignCounts, IpcDesignTreeSnapshot,
    IpcDispatcherSnapshot, IpcFocusedNode, IpcHarnessSnapshot, IpcHealthSnapshot, IpcHealthState,
    IpcMemorySnapshot, IpcNodeBrief, IpcOpenSpecSnapshot, IpcOperationEpisodeSnapshot,
    IpcPresentationSnapshot, IpcProviderSnapshot, IpcSessionSnapshot, IpcStateSnapshot,
    OmegonAutonomyMode, OmegonControlPlane, OmegonDeploymentKind, OmegonIdentity,
    OmegonInstanceDescriptor, OmegonOwnerKind, OmegonOwnership, OmegonPlacement,
    OmegonPlacementKind, OmegonRole, OmegonRuntime, OmegonRuntimeHealth, OmegonRuntimeProfile,
};

use crate::runtime_state::RuntimeStateHandles as DashboardHandles;

/// Build a full state snapshot from the shared dashboard handles.
/// Always returns a valid snapshot even if some handles are unavailable.
pub fn build_state_snapshot(
    handles: &DashboardHandles,
    omegon_version: &str,
    cwd: &str,
    started_at: &str,
    server_instance_id: &str,
    session_view_binding: &crate::session_consumers::SessionViewBinding,
    presentation_level: crate::surfaces::layout::UiPresentationLevel,
) -> IpcStateSnapshot {
    let session = project_session(handles, cwd, started_at, session_view_binding);
    let design_tree = project_design_tree(handles);
    let openspec = project_openspec(handles);
    let cleave = project_cleave(handles);
    let harness = project_harness(handles);
    let health = project_health(handles);
    let instance = project_instance(
        handles,
        cwd,
        &session,
        &harness,
        &health,
        omegon_version,
        server_instance_id,
    );

    IpcStateSnapshot {
        schema_version: omegon_traits::IPC_PROTOCOL_VERSION,
        omegon_version: omegon_version.to_string(),
        instance,
        session,
        design_tree,
        openspec,
        cleave,
        harness,
        health,
        presentation: Some(ipc_presentation_snapshot(presentation_level)),
        operation_episodes: ipc_operation_episodes(handles),
        runtime_lifecycle: handles.observe_runtime_lifecycle().ok().flatten(),
    }
}

fn ipc_operation_episodes(handles: &DashboardHandles) -> Vec<IpcOperationEpisodeSnapshot> {
    let mut episodes = Vec::new();
    if let Ok(Some(progress)) = handles.observe_delegate()
        && (progress.active || progress.running > 0)
    {
        let projection = crate::features::operation_surface::project_delegate(&progress);
        episodes.push(ipc_operation_episode(&projection));
    }
    if let Ok(Some(progress)) = handles.observe_cleave()
        && progress.active
    {
        let projection = crate::features::operation_surface::project_cleave(&progress);
        episodes.push(ipc_operation_episode(&projection));
    }
    episodes
}

fn ipc_operation_episode(
    projection: &crate::surfaces::operations::OperationWorkbenchProjection,
) -> IpcOperationEpisodeSnapshot {
    let kind = match projection.operation.kind {
        omegon_traits::OperationKind::Delegate => "delegate",
        omegon_traits::OperationKind::Cleave => "cleave",
    };
    let raw_id = projection.operation.id.as_deref().unwrap_or("active");
    let total = projection.children.len();
    let state = if projection.failed > 0 {
        "failed"
    } else if projection.running > 0 {
        "running"
    } else {
        "complete"
    };
    IpcOperationEpisodeSnapshot {
        id: format!("{kind}:{raw_id}"),
        kind: kind.into(),
        state: state.into(),
        outcome: format!(
            "{kind} · {} done · {} running · {} failed",
            projection.completed, projection.running, projection.failed
        ),
        evidence_refs: projection
            .children
            .iter()
            .map(|child| format!("{kind}:child:{}", child.id))
            .collect(),
        completed: projection.completed,
        total,
    }
}

fn ipc_presentation_snapshot(
    level: crate::surfaces::layout::UiPresentationLevel,
) -> IpcPresentationSnapshot {
    let policy = crate::surfaces::layout::UiPresentationPolicy::named(level);
    IpcPresentationSnapshot {
        level: level.name().to_string(),
        preset: policy.preset_name().to_string(),
        transcript_density: match policy.transcript_density() {
            crate::surfaces::layout::TranscriptDensity::Outcomes => "outcomes",
            crate::surfaces::layout::TranscriptDensity::Evidence => "evidence",
        }
        .to_string(),
        live_detail: match policy.live_detail() {
            crate::surfaces::layout::LiveDetail::Status => "status",
            crate::surfaces::layout::LiveDetail::Workflow => "workflow",
            crate::surfaces::layout::LiveDetail::Diagnostic => "diagnostic",
        }
        .to_string(),
        telemetry_density: match policy.telemetry_density() {
            crate::surfaces::layout::TelemetryDensity::Essential => "essential",
            crate::surfaces::layout::TelemetryDensity::Operational => "operational",
            crate::surfaces::layout::TelemetryDensity::Diagnostic => "diagnostic",
        }
        .to_string(),
        supported_levels: vec!["om".into(), "active".into(), "full".into()],
        dashboard: policy.surfaces.dashboard,
        instruments: policy.surfaces.instruments,
        footer: policy.surfaces.footer,
        activity: policy.surfaces.activity,
    }
}

fn project_session(
    handles: &DashboardHandles,
    cwd: &str,
    started_at: &str,
    session_view_binding: &crate::session_consumers::SessionViewBinding,
) -> IpcSessionSnapshot {
    let stats = handles.session().observe().unwrap_or_default();
    let target = session_view_binding.snapshot();
    let semantic = crate::session_consumers::SemanticSessionView::load(&target).ok();
    let frontend = semantic.as_ref().and_then(|view| view.frontend.as_ref());
    let runtime_queue = session_view_binding.runtime_queue_snapshot();
    let durable_queue_depth = frontend.map_or(0, |snapshot| snapshot.queued_prompts.len());
    let queue_depth = runtime_queue["depth"]
        .as_u64()
        .map_or(durable_queue_depth, |depth| depth as usize);
    let runtime_busy = runtime_queue
        .get("active")
        .map(|active| !active.is_null())
        .unwrap_or(stats.busy);

    let (git_branch, git_detached) = handles
        .observe_harness()
        .ok()
        .flatten()
        .map(|status| (status.git_branch, status.git_detached))
        .unwrap_or((None, false));

    IpcSessionSnapshot {
        cwd: cwd.to_string(),
        pid: std::process::id(),
        started_at: started_at.to_string(),
        turns: stats.turns,
        tool_calls: stats.tool_calls,
        compactions: stats.compactions,
        busy: runtime_busy,
        git_branch,
        git_detached,
        session_id: Some(target.session_id),
        session_generation: Some(target.generation),
        stream_id: semantic.as_ref().map(|view| view.stream_id.to_string()),
        projection_status: Some(
            match semantic.as_ref().map(|view| view.status) {
                Some(crate::session_consumers::SemanticSessionStatus::ExactFull) => "exact_full",
                Some(crate::session_consumers::SemanticSessionStatus::ExactSuffix) => {
                    "exact_suffix"
                }
                Some(crate::session_consumers::SemanticSessionStatus::LegacyUnavailable) => {
                    "legacy_unavailable"
                }
                None => "unavailable",
            }
            .into(),
        ),
        projection_frontier: semantic.as_ref().map(|view| view.frontier_sequence),
        context_revision: frontend.map(|snapshot| snapshot.context.context_revision),
        queue_depth,
        active_turn: Some(
            if let Some(active) = runtime_queue.get("active") {
                if active.is_null() { "idle" } else { "active" }
            } else {
                frontend
                    .map(crate::session_consumers::active_turn_label)
                    .unwrap_or("idle")
            }
            .into(),
        ),
    }
}

pub(crate) fn project_design_tree(handles: &DashboardHandles) -> IpcDesignTreeSnapshot {
    let Ok(observation) = handles.lifecycle_service.observe() else {
        return IpcDesignTreeSnapshot {
            counts: IpcDesignCounts::default(),
            focused: None,
            implementing: vec![],
            actionable: vec![],
            nodes: vec![],
        };
    };
    let Some(repository) = observation.repository else {
        return IpcDesignTreeSnapshot {
            counts: IpcDesignCounts::default(),
            focused: None,
            implementing: vec![],
            actionable: vec![],
            nodes: vec![],
        };
    };

    use crate::lifecycle::types::NodeStatus;

    let all = &repository.design.nodes;
    let mut counts = IpcDesignCounts {
        total: all.len(),
        ..IpcDesignCounts::default()
    };

    let mut nodes = Vec::with_capacity(all.len());
    let mut implementing = vec![];
    let mut actionable = vec![];

    for node in all.values() {
        match node.status {
            NodeStatus::Seed => counts.seed += 1,
            NodeStatus::Exploring => counts.exploring += 1,
            NodeStatus::Resolved => counts.resolved += 1,
            NodeStatus::Decided => counts.decided += 1,
            NodeStatus::Implementing => counts.implementing += 1,
            NodeStatus::Implemented => counts.implemented += 1,
            NodeStatus::Blocked => counts.blocked += 1,
            NodeStatus::Deferred | NodeStatus::Archived => counts.deferred += 1,
        }
        counts.open_questions += node.open_questions.len();

        let brief = IpcNodeBrief {
            id: node.id.clone(),
            title: node.title.clone(),
            status: node.status.as_str().to_string(),
            parent: node.parent.clone(),
            open_questions: node.open_questions.len(),
            tags: node.tags.clone(),
        };

        if node.status == NodeStatus::Implementing {
            implementing.push(brief.clone());
        }
        if matches!(node.status, NodeStatus::Exploring | NodeStatus::Decided)
            && !node.open_questions.is_empty()
        {
            actionable.push(brief.clone());
        }
        nodes.push(brief);
    }

    let focused = observation
        .focus
        .node_id
        .as_deref()
        .and_then(|id| all.get(id))
        .map(|n| IpcFocusedNode {
            id: n.id.clone(),
            title: n.title.clone(),
            status: n.status.as_str().to_string(),
            open_questions: n.open_questions.clone(),
            decisions: repository
                .sections
                .get(&n.id)
                .map(|sections| sections.decisions.len())
                .unwrap_or(0),
            children: all
                .values()
                .filter(|c| c.parent.as_deref() == Some(&n.id))
                .count(),
        });

    IpcDesignTreeSnapshot {
        counts,
        focused,
        implementing,
        actionable,
        nodes,
    }
}

pub(crate) fn project_openspec(handles: &DashboardHandles) -> IpcOpenSpecSnapshot {
    let Ok(observation) = handles.lifecycle_service.observe() else {
        return IpcOpenSpecSnapshot {
            changes: vec![],
            total_tasks: 0,
            done_tasks: 0,
        };
    };
    let Some(repository) = observation.repository else {
        return IpcOpenSpecSnapshot {
            changes: vec![],
            total_tasks: 0,
            done_tasks: 0,
        };
    };
    let openspec = &repository.lifecycle.openspec;

    let changes: Vec<IpcChangeSnapshot> = openspec
        .changes
        .iter()
        .map(|c| IpcChangeSnapshot {
            name: c.name.clone(),
            stage: c.lifecycle_state.clone(),
            total_tasks: c.total_tasks,
            done_tasks: c.done_tasks,
        })
        .collect();

    IpcOpenSpecSnapshot {
        changes,
        total_tasks: openspec.total_tasks,
        done_tasks: openspec.done_tasks,
    }
}

fn project_cleave(handles: &DashboardHandles) -> IpcCleaveSnapshot {
    let cp = handles.observe_cleave().ok().flatten();
    let Some(cp) = cp else {
        return IpcCleaveSnapshot {
            active: false,
            total_children: 0,
            completed: 0,
            failed: 0,
            children: vec![],
        };
    };

    IpcCleaveSnapshot {
        active: cp.active,
        total_children: cp.total_children,
        completed: cp.completed,
        failed: cp.failed,
        children: cp
            .children
            .iter()
            .map(|c| IpcChildSnapshot {
                label: c.label.clone(),
                status: c.status.clone(),
                duration_secs: c.duration_secs,
            })
            .collect(),
    }
}

pub fn project_instance_descriptor(
    handles: &DashboardHandles,
    cwd: &str,
    session: &IpcSessionSnapshot,
    harness: &IpcHarnessSnapshot,
    health: &IpcHealthSnapshot,
    omegon_version: &str,
    server_instance_id: &str,
) -> OmegonInstanceDescriptor {
    let host = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("HOST").ok());
    let workspace_id = workspace_id_from_cwd(cwd);
    let auth = handles
        .observe_harness()
        .ok()
        .flatten()
        .map(|h| (h.web_auth_mode, h.web_auth_source));

    OmegonInstanceDescriptor {
        schema_version: omegon_traits::IPC_PROTOCOL_VERSION,
        identity: OmegonIdentity {
            instance_id: server_instance_id.to_string(),
            workspace_id,
            session_id: session
                .session_id
                .clone()
                .unwrap_or_else(|| "detached".into()),
            role: OmegonRole::PrimaryDriver,
            profile: harness.runtime_profile.clone(),
        },
        ownership: OmegonOwnership {
            owner_kind: OmegonOwnerKind::Operator,
            owner_id: "local-terminal".into(),
            parent_instance_id: None,
        },
        placement: OmegonPlacement {
            kind: OmegonPlacementKind::LocalProcess,
            host,
            pid: Some(std::process::id()),
            cwd: cwd.to_string(),
            namespace: None,
            pod_name: None,
            container_name: None,
        },
        control_plane: OmegonControlPlane {
            server_instance_id: server_instance_id.to_string(),
            protocol_version: omegon_traits::IPC_PROTOCOL_VERSION,
            schema_version: omegon_traits::IPC_PROTOCOL_VERSION,
            omegon_version: omegon_version.to_string(),
            capabilities: omegon_traits::IpcCapability::v1_server_set()
                .into_iter()
                .map(str::to_string)
                .collect(),
            ipc_socket_path: Some(
                std::path::Path::new(cwd)
                    .join(".omegon")
                    .join("ipc.sock")
                    .display()
                    .to_string(),
            ),
            http_base: None,
            startup_url: None,
            state_url: None,
            ws_url: None,
            auth_mode: auth.as_ref().and_then(|(mode, _)| mode.clone()),
            auth_source: auth.as_ref().and_then(|(_, source)| source.clone()),
            http_transport_security: None,
            ws_transport_security: None,
        },
        runtime: OmegonRuntime {
            deployment_kind: OmegonDeploymentKind::InteractiveTui,
            runtime_mode: omegon_traits::OmegonRuntimeMode::Standalone,
            runtime_profile: OmegonRuntimeProfile::PrimaryInteractive,
            autonomy_mode: OmegonAutonomyMode::OperatorDriven,
            health: match health.state {
                IpcHealthState::Ready => OmegonRuntimeHealth::Ready,
                IpcHealthState::Degraded => OmegonRuntimeHealth::Degraded,
                IpcHealthState::Starting => OmegonRuntimeHealth::Starting,
                IpcHealthState::Failed => OmegonRuntimeHealth::Failed,
            },
            provider_ok: health.provider_ok,
            memory_ok: health.memory_ok,
            cleave_available: harness.cleave_available,
            queued_events: 0,
            transport_warnings: vec![],
            runtime_dir: None,
            context_class: Some(harness.context_class.clone()),
            thinking_level: Some(harness.thinking_level.clone()),
            capability_tier: Some(harness.capability_tier.clone()),
            execution_substrate: harness.execution_substrate.clone(),
        },
    }
}

fn project_instance(
    handles: &DashboardHandles,
    cwd: &str,
    session: &IpcSessionSnapshot,
    harness: &IpcHarnessSnapshot,
    health: &IpcHealthSnapshot,
    omegon_version: &str,
    server_instance_id: &str,
) -> OmegonInstanceDescriptor {
    project_instance_descriptor(
        handles,
        cwd,
        session,
        harness,
        health,
        omegon_version,
        server_instance_id,
    )
}

fn workspace_id_from_cwd(cwd: &str) -> String {
    let trimmed = cwd.trim_matches('/');
    if trimmed.is_empty() {
        return "root".into();
    }
    trimmed.replace('/', "::")
}

fn project_harness(handles: &DashboardHandles) -> IpcHarnessSnapshot {
    let Ok(Some(h)) = handles.observe_harness() else {
        return IpcHarnessSnapshot {
            context_class: "Compact".into(),
            thinking_level: "Medium".into(),
            capability_tier: "B".into(),
            runtime_profile: "primary-interactive".into(),
            autonomy_mode: "operator-driven".into(),
            dispatcher: IpcDispatcherSnapshot {
                available_options: vec![
                    "F".into(),
                    "D".into(),
                    "C".into(),
                    "B".into(),
                    "A".into(),
                    "S".into(),
                ],
                switch_state: "idle".into(),
                request_id: None,
                expected_profile: None,
                expected_model: None,
                active_profile: Some("B".into()),
                active_model: None,
                failure_code: None,
                note: None,
            },
            memory_available: false,
            cleave_available: false,
            memory_warning: None,
            memory: IpcMemorySnapshot {
                active_facts: 0,
                project_facts: 0,
                working_facts: 0,
                episodes: 0,
            },
            providers: vec![],
            mcp_server_count: 0,
            mcp_tool_count: 0,
            active_persona: None,
            active_tone: None,
            active_delegate_count: 0,
            execution_substrate: Some(crate::execution_substrate::detect()),
        };
    };
    IpcHarnessSnapshot {
        context_class: h.context_class.clone(),
        thinking_level: h.thinking_level.clone(),
        capability_tier: h.capability_grade.clone(),
        runtime_profile: h.runtime_profile.as_str().to_string(),
        autonomy_mode: match h.autonomy_mode {
            omegon_traits::OmegonAutonomyMode::OperatorDriven => "operator-driven".into(),
            omegon_traits::OmegonAutonomyMode::GuardedAutonomous => "guarded-autonomous".into(),
            omegon_traits::OmegonAutonomyMode::Autonomous => "autonomous".into(),
        },
        dispatcher: IpcDispatcherSnapshot {
            available_options: h.dispatcher.available_options.clone(),
            switch_state: h.dispatcher.switch_state.clone(),
            request_id: h.dispatcher.request_id.clone(),
            expected_profile: h.dispatcher.expected_profile.clone(),
            expected_model: h.dispatcher.expected_model.clone(),
            active_profile: h.dispatcher.active_profile.clone(),
            active_model: h.dispatcher.active_model.clone(),
            failure_code: h.dispatcher.failure_code.clone(),
            note: h.dispatcher.note.clone(),
        },
        memory_available: h.memory_available,
        cleave_available: h.cleave_available,
        memory_warning: h.memory_warning.clone(),
        memory: IpcMemorySnapshot {
            active_facts: h.memory.active_facts,
            project_facts: h.memory.project_facts,
            working_facts: h.memory.working_facts,
            episodes: h.memory.episodes,
        },
        providers: h
            .providers
            .iter()
            .map(|p| IpcProviderSnapshot {
                name: p.name.clone(),
                authenticated: p.authenticated,
                model: p.model.clone(),
                runtime_status: p.runtime_status.map(|s| format!("{:?}", s).to_lowercase()),
                recent_failure_count: p.recent_failure_count,
                last_failure_kind: p.last_failure_kind.clone(),
            })
            .collect(),
        mcp_server_count: h.mcp_servers.iter().filter(|s| s.connected).count(),
        mcp_tool_count: h.mcp_tool_count(),
        active_persona: h.active_persona.as_ref().map(|p| p.name.clone()),
        active_tone: h.active_tone.as_ref().map(|t| t.name.clone()),
        active_delegate_count: h.active_delegates.len(),
        execution_substrate: Some(h.execution_substrate.clone()),
    }
}

fn project_health(handles: &DashboardHandles) -> IpcHealthSnapshot {
    let now = chrono::Utc::now().to_rfc3339();
    let (memory_ok, provider_ok) = handles
        .observe_harness()
        .ok()
        .flatten()
        .map(|h| {
            let mem_ok = h.memory_available || h.memory_warning.is_none();
            let prov_ok = h.providers.iter().any(|p| {
                p.authenticated
                    && !matches!(
                        p.runtime_status,
                        Some(crate::status::ProviderRuntimeStatus::Degraded)
                    )
            });
            (mem_ok, prov_ok)
        })
        .unwrap_or((true, false));

    IpcHealthSnapshot {
        state: IpcHealthState::Ready,
        memory_ok,
        provider_ok,
        checked_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn binding() -> crate::session_consumers::SessionViewBinding {
        crate::session_consumers::SessionViewBinding::new(
            std::path::PathBuf::from("/tmp/example-project/session-abc.json"),
            "session-abc".into(),
        )
    }

    #[test]
    fn build_state_snapshot_retains_latest_runtime_lifecycle() {
        let lifecycle = omegon_traits::RuntimeLifecycleSnapshot {
            operation_id: "restart-1".into(),
            kind: omegon_traits::RuntimeLifecycleKind::Restart,
            phase: omegon_traits::RuntimeLifecyclePhase::Restarting,
            message: "Saving session and restarting".into(),
            session_id: Some("session-abc".into()),
            target_version: None,
            reconnect_required: true,
        };
        let handles = DashboardHandles::default();
        handles
            .publish_runtime_lifecycle(lifecycle.clone(), |_| {})
            .unwrap();

        let snap = build_state_snapshot(
            &handles,
            "0.28.0",
            "/tmp/example-project",
            "2026-07-12T12:00:00Z",
            "instance-123",
            &binding(),
            crate::surfaces::layout::UiPresentationLevel::Om,
        );

        assert_eq!(snap.runtime_lifecycle, Some(lifecycle));
        assert!(
            snap.instance
                .control_plane
                .capabilities
                .contains(&"runtime.lifecycle".to_string())
        );
    }

    #[test]
    fn build_state_snapshot_includes_instance_descriptor() {
        let handles = DashboardHandles::default();
        handles.install_harness(Arc::new(Mutex::new(crate::status::HarnessStatus {
            context_class: "Compact".into(),
            thinking_level: "high".into(),
            capability_grade: "B".into(),
            runtime_profile: omegon_traits::OmegonRuntimeProfile::PrimaryInteractive,
            autonomy_mode: omegon_traits::OmegonAutonomyMode::OperatorDriven,
            dispatcher: crate::status::DispatcherStatus {
                available_options: vec!["D".into(), "B".into(), "S".into()],
                switch_state: "idle".into(),
                request_id: None,
                expected_profile: None,
                expected_model: None,
                active_profile: Some("B".into()),
                active_model: Some("anthropic:claude-sonnet-4-6".into()),
                failure_code: None,
                note: None,
            },
            memory_available: true,
            cleave_available: true,
            web_auth_mode: Some("ephemeral-bearer".into()),
            web_auth_source: Some("generated".into()),
            ..Default::default()
        })));

        let snap = build_state_snapshot(
            &handles,
            "0.15.10-rc.15",
            "/tmp/example-project",
            "2026-04-05T12:00:00Z",
            "instance-123",
            &binding(),
            crate::surfaces::layout::UiPresentationLevel::Active,
        );

        assert_eq!(snap.instance.identity.instance_id, "instance-123");
        assert_eq!(snap.instance.identity.session_id, "session-abc");
        let presentation = snap.presentation.expect("presentation snapshot");
        assert_eq!(presentation.level, "active");
        assert_eq!(presentation.live_detail, "workflow");
        assert_eq!(snap.instance.identity.workspace_id, "tmp::example-project");
        assert_eq!(snap.instance.identity.profile, "primary-interactive");
        assert_eq!(snap.harness.runtime_profile, "primary-interactive");
        assert_eq!(snap.harness.autonomy_mode, "operator-driven");
        assert_eq!(snap.harness.dispatcher.switch_state, "idle");
        assert_eq!(snap.harness.dispatcher.active_profile.as_deref(), Some("B"));
        assert_eq!(
            snap.harness.dispatcher.active_model.as_deref(),
            Some("anthropic:claude-sonnet-4-6")
        );
        assert_eq!(
            snap.instance.control_plane.server_instance_id,
            "instance-123"
        );
        assert_eq!(
            snap.instance.control_plane.schema_version,
            omegon_traits::IPC_PROTOCOL_VERSION
        );
        assert_eq!(snap.instance.control_plane.omegon_version, "0.15.10-rc.15");
        assert_eq!(snap.session.session_id.as_deref(), Some("session-abc"));
        assert_eq!(
            snap.instance.runtime.thinking_level.as_deref(),
            Some("high")
        );
        assert!(snap.harness.execution_substrate.is_some());
        assert_eq!(
            snap.instance.runtime.execution_substrate,
            snap.harness.execution_substrate
        );
    }

    #[test]
    fn state_snapshot_reads_dynamic_session_binding_and_queue() {
        let handles = DashboardHandles::default();
        handles.session().set_busy(false);
        let binding = binding();
        binding.update_runtime_queue(serde_json::json!({"depth": 1, "active": {"turn_id": 2}}));

        let first = build_state_snapshot(
            &handles,
            "0.29.0",
            "/tmp/example-project",
            "2026-08-22T00:00:00Z",
            "instance-123",
            &binding,
            crate::surfaces::layout::UiPresentationLevel::Om,
        );
        assert_eq!(first.session.session_id.as_deref(), Some("session-abc"));
        assert_eq!(first.session.queue_depth, 1);
        assert!(first.session.busy);

        handles.session().set_busy(true);
        binding.update_runtime_queue(serde_json::json!({"depth": 0, "active": null, "items": []}));
        let idle = build_state_snapshot(
            &handles,
            "0.29.0",
            "/tmp/example-project",
            "2026-08-22T00:00:00Z",
            "instance-123",
            &binding,
            crate::surfaces::layout::UiPresentationLevel::Om,
        );
        assert!(!idle.session.busy);
        assert_eq!(idle.session.active_turn.as_deref(), Some("idle"));

        let mut replacement = binding.snapshot();
        replacement.session_id = "session-next".into();
        replacement.generation += 1;
        binding.replace(replacement);
        let second = build_state_snapshot(
            &handles,
            "0.29.0",
            "/tmp/example-project",
            "2026-08-22T00:00:00Z",
            "instance-123",
            &binding,
            crate::surfaces::layout::UiPresentationLevel::Om,
        );
        assert_eq!(second.session.session_id.as_deref(), Some("session-next"));
        assert_eq!(second.session.session_generation, Some(2));
        assert_eq!(second.session.queue_depth, 0);
    }
}
