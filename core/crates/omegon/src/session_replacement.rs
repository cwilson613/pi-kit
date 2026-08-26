//! Idle-only atomic replacement of an authority-backed host session.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::conversation::ConversationState;
use crate::runtime_supervisor::InteractiveRuntimeSupervisor;
use crate::session_authority::{ActorIdentity, SessionAuthority, SessionAuthorityHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionReplacementKind {
    Resume,
    New,
    ContextClear,
}

pub(crate) struct SessionReplacementRequest {
    pub(crate) kind: SessionReplacementKind,
    pub(crate) target_session_id: String,
    pub(crate) target_snapshot: PathBuf,
    pub(crate) target_conversation: ConversationState,
    pub(crate) target_meta: Option<crate::session::SessionMeta>,
    pub(crate) resume_info: Option<crate::setup::ResumeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionBinding {
    pub(crate) session_id: String,
    pub(crate) stream_id: Uuid,
    pub(crate) last_sequence: u64,
    pub(crate) last_event_id: Uuid,
    pub(crate) context_revision: u64,
}

impl ProjectionBinding {
    pub(crate) fn from_authority(
        authority: &SessionAuthorityHandle,
    ) -> Result<Self, SessionReplacementError> {
        let state = authority.state();
        Ok(Self {
            session_id: state.session_id.ok_or_else(|| {
                SessionReplacementError::Target("authority has no session identity".into())
            })?,
            stream_id: state.stream_id.ok_or_else(|| {
                SessionReplacementError::Target("authority has no stream identity".into())
            })?,
            last_sequence: state.last_sequence,
            last_event_id: state.last_event_id.ok_or_else(|| {
                SessionReplacementError::Target("authority has no frontier event".into())
            })?,
            context_revision: state.context_revision,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SessionReplacementRejection {
    #[error("an active turn or executing worker owns the host")]
    ActiveTurn,
    #[error("queued prompts have ownership that cannot be moved safely")]
    QueuedPrompts,
    #[error("an invocation has not reached a terminal state")]
    UnresolvedInvocation,
    #[error("compaction or context replacement is unresolved")]
    ActiveCompaction,
    #[error("execution-binding migration is pending")]
    ExecutionBindingMigration,
    #[error("the target is already the active session")]
    UnchangedTarget,
    #[error("sessionless hosts do not replace semantic session authority")]
    Sessionless,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SessionReplacementError {
    #[error("session replacement rejected: {0}")]
    Rejected(#[from] SessionReplacementRejection),
    #[error("target session validation failed: {0}")]
    Target(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionReplacementOutcome {
    pub(crate) kind: SessionReplacementKind,
    pub(crate) previous_session_id: String,
    pub(crate) session_id: String,
    pub(crate) host_generation: u64,
    pub(crate) projection: ProjectionBinding,
}

pub(crate) fn emit_canonical_session_start(
    bus: &mut crate::bus::EventBus,
    cwd: &Path,
    outcome: &SessionReplacementOutcome,
) {
    bus.emit(&omegon_traits::BusEvent::SessionStart {
        session_id: outcome.session_id.clone(),
        cwd: cwd.to_path_buf(),
    });
}

pub(crate) struct HostSessionPublication<'a> {
    pub(crate) supervisor: &'a mut InteractiveRuntimeSupervisor,
    pub(crate) conversation: &'a mut ConversationState,
    pub(crate) displayed_session_id: &'a mut String,
    pub(crate) resume_info: &'a mut Option<crate::setup::ResumeInfo>,
}

pub(crate) struct SessionReplacementEnvironment<'a> {
    pub(crate) cwd: &'a Path,
    pub(crate) persist_current: bool,
    pub(crate) workspace_identity: &'a str,
    pub(crate) runtime_generation_id: &'a str,
    pub(crate) actor: ActorIdentity,
}

pub(crate) fn fresh_request(
    kind: SessionReplacementKind,
    cwd: &Path,
) -> Result<SessionReplacementRequest, SessionReplacementError> {
    debug_assert!(matches!(
        kind,
        SessionReplacementKind::New | SessionReplacementKind::ContextClear
    ));
    let target_session_id = crate::session::allocate_session_id();
    let directory = crate::session::sessions_dir(cwd).ok_or_else(|| {
        SessionReplacementError::Target("cannot determine session directory".into())
    })?;
    Ok(SessionReplacementRequest {
        kind,
        target_snapshot: directory.join(format!("{target_session_id}.json")),
        target_session_id,
        target_conversation: ConversationState::new(),
        target_meta: None,
        resume_info: None,
    })
}

pub(crate) fn resume_request(
    cwd: &Path,
    path: &Path,
) -> Result<SessionReplacementRequest, SessionReplacementError> {
    let (conversation, meta) = crate::session::load_for_resume(cwd, path)
        .map_err(|error| SessionReplacementError::Target(error.to_string()))?;
    if conversation.turn_count() != meta.turns
        || conversation.intent.stats.tool_calls != meta.tool_calls
    {
        return Err(SessionReplacementError::Target(
            "compatibility snapshot does not match its session metadata".into(),
        ));
    }
    let description = crate::session::session_display_description(&meta);
    Ok(SessionReplacementRequest {
        kind: SessionReplacementKind::Resume,
        target_session_id: meta.session_id.clone(),
        target_snapshot: path.to_path_buf(),
        target_conversation: conversation,
        target_meta: Some(meta.clone()),
        resume_info: Some(crate::setup::ResumeInfo {
            session_id: meta.session_id,
            turns: meta.turns,
            description,
            last_prompt_snippet: meta.last_prompt_snippet,
            created_at: meta.created_at,
        }),
    })
}

pub(crate) fn replace(
    publication: HostSessionPublication<'_>,
    request: SessionReplacementRequest,
    environment: SessionReplacementEnvironment<'_>,
) -> Result<SessionReplacementOutcome, SessionReplacementError> {
    publication.supervisor.replacement_quiescence()?;
    if publication.supervisor.invocation_authority().is_none() {
        return Err(SessionReplacementRejection::Sessionless.into());
    }
    if *publication.displayed_session_id == request.target_session_id {
        return Err(SessionReplacementRejection::UnchangedTarget.into());
    }

    // Everything through candidate construction is fallible. The live host is
    // untouched until authority recovery, blob validation, replay, and binding
    // validation have all succeeded.
    let target_had_authority = crate::session_host_storage::has_authority(&request.target_snapshot)
        .map_err(|error| SessionReplacementError::Target(error.to_string()))?;
    let recorded_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let authority = SessionAuthority::open(
        &request.target_snapshot,
        &request.target_session_id,
        environment.workspace_identity,
        environment.runtime_generation_id,
        environment.actor,
        &recorded_at,
    )
    .map_err(|error| SessionReplacementError::Target(error.to_string()))?;
    let mut candidate = InteractiveRuntimeSupervisor::with_authority(authority)
        .map_err(|error| SessionReplacementError::Target(error.to_string()))?;
    candidate.replacement_quiescence()?;
    let authority = candidate
        .invocation_authority()
        .expect("authority-backed replacement candidate");
    let mut projection = ProjectionBinding::from_authority(&authority)?;
    crate::session_replay::SessionReplay::replay_prefix(
        &request.target_snapshot,
        &request.target_session_id,
        projection.stream_id,
        crate::session_replay::ReplayEnd::EndOfStream,
    )
    .map_err(|error| SessionReplacementError::Target(error.to_string()))?;
    if target_had_authority {
        crate::session_host_storage::validate_replacement_target(
            &request.target_snapshot,
            &request.target_session_id,
            environment.cwd,
            &projection,
        )
        .map_err(|error| SessionReplacementError::Target(error.to_string()))?;
    }
    if let Some(metadata) = request.target_meta.as_ref()
        && authority
            .import_legacy_compatibility_base(
                &request.target_conversation.build_llm_view(),
                &recorded_at,
            )
            .map_err(|error| SessionReplacementError::Target(error.to_string()))?
    {
        let binding = crate::session_host_storage::SessionStorageBinding::from_authority(
            &request.target_snapshot,
            &request.target_session_id,
            Some(&authority),
            environment.cwd,
        );
        crate::session_host_storage::save_full_spine(
            &binding,
            &request.target_conversation,
            Some(metadata),
        )
        .map_err(|error| SessionReplacementError::Target(error.to_string()))?;
        projection = ProjectionBinding::from_authority(&authority)?;
    }

    let previous_session_id = publication.displayed_session_id.clone();
    let host_generation = publication
        .supervisor
        .host_session_generation()
        .checked_add(1)
        .ok_or_else(|| {
            SessionReplacementError::Target("host session generation overflow".into())
        })?;
    candidate.publish_replacement_generation(host_generation);

    if environment.persist_current {
        crate::session::save_session(
            publication.conversation,
            environment.cwd,
            Some(publication.displayed_session_id.as_str()),
        )
        .map_err(|error| {
            SessionReplacementError::Target(format!("current session could not be saved: {error}"))
        })?;
    }

    publication.supervisor.drain_shadow_projection_worker();
    *publication.supervisor = candidate;
    *publication.conversation = request.target_conversation;
    *publication.displayed_session_id = request.target_session_id.clone();
    *publication.resume_info = request.resume_info;

    Ok(SessionReplacementOutcome {
        kind: request.kind,
        previous_session_id,
        session_id: request.target_session_id,
        host_generation,
        projection,
    })
}

pub(crate) fn replace_sessionless(
    conversation: &mut ConversationState,
    displayed_session_id: &mut String,
    resume_info: &mut Option<crate::setup::ResumeInfo>,
    request: SessionReplacementRequest,
) -> SessionReplacementOutcome {
    let previous_session_id = displayed_session_id.clone();
    *conversation = request.target_conversation;
    *displayed_session_id = request.target_session_id.clone();
    *resume_info = request.resume_info;
    SessionReplacementOutcome {
        kind: request.kind,
        previous_session_id,
        session_id: request.target_session_id.clone(),
        host_generation: 0,
        projection: ProjectionBinding {
            session_id: request.target_session_id,
            stream_id: Uuid::nil(),
            last_sequence: 0,
            last_event_id: Uuid::nil(),
            context_revision: 0,
        },
    }
}

#[cfg(test)]
mod source_guards {
    #[test]
    fn command_handlers_cannot_directly_swap_session_components() {
        for source in [
            include_str!("control_runtime.rs"),
            include_str!("acp_worker.rs"),
            include_str!("session_commands.rs"),
        ] {
            assert!(!source.contains("runtime_state.conversation ="));
            assert!(!source.contains("conversation = loaded"));
            assert!(!source.contains("supervisor = restored"));
            assert!(!source.contains("agent.session_id ="));
        }
        let acp = include_str!("acp_worker.rs");
        assert!(acp.contains("crate::session_replacement::replace("));
        assert!(acp.contains("SessionReplacementKind::ContextClear"));
        assert!(acp.contains("WorkerRequest::LoadSession"));
        let transport = include_str!("acp.rs");
        let identity_publish = transport
            .find("*self.session_id.borrow_mut() = Some(args.session_id.clone())")
            .expect("ACP publishes replacement identity");
        let replay_publish = transport
            .find("for message in replay.messages")
            .expect("ACP publishes semantic history");
        assert!(identity_publish < replay_publish);
        assert!(!acp.contains("target_conversation.replay_messages"));
        assert_eq!(
            acp.matches("emit_canonical_session_start").count(),
            2,
            "ACP resume and new-session replacement each publish once"
        );
        assert_eq!(
            include_str!("control_runtime.rs")
                .matches("emit_canonical_session_start")
                .count(),
            2,
            "interactive sessionless and authority-backed replacement each publish once"
        );
        assert!(include_str!("main.rs").contains("emit_canonical_session_start"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_prompt::{ControlSurface, QueueMode, RuntimeActor};
    use std::{thread, time::Duration};

    struct SessionObserver(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

    #[async_trait::async_trait]
    impl omegon_traits::Feature for SessionObserver {
        fn name(&self) -> &str {
            "session-observer"
        }

        fn on_event(&mut self, event: &omegon_traits::BusEvent) -> Vec<omegon_traits::BusRequest> {
            if let omegon_traits::BusEvent::SessionStart { session_id, .. } = event {
                self.0.lock().unwrap().push(session_id.clone());
            }
            Vec::new()
        }
    }

    #[test]
    fn canonical_replacement_event_reaches_retained_features_once() {
        let observed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(SessionObserver(observed.clone())));
        bus.finalize();
        let outcome = SessionReplacementOutcome {
            kind: SessionReplacementKind::New,
            previous_session_id: "old-session".into(),
            session_id: "new-session".into(),
            host_generation: 2,
            projection: ProjectionBinding {
                session_id: "new-session".into(),
                stream_id: Uuid::nil(),
                last_sequence: 0,
                last_event_id: Uuid::nil(),
                context_revision: 0,
            },
        };
        emit_canonical_session_start(&mut bus, Path::new("."), &outcome);
        assert_eq!(observed.lock().unwrap().as_slice(), ["new-session"]);
    }

    const WORKSPACE: &str = "workspace-1";
    const GENERATION: &str = "generation-1";

    struct Host {
        _dir: tempfile::TempDir,
        cwd: PathBuf,
        supervisor: InteractiveRuntimeSupervisor,
        conversation: ConversationState,
        session_id: String,
        resume_info: Option<crate::setup::ResumeInfo>,
    }

    fn actor() -> ActorIdentity {
        ActorIdentity {
            principal: "test".into(),
            ingress: "test".into(),
        }
    }

    fn host() -> Host {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        let session_id = "2026-08-21T12-00-00_00000001".to_string();
        let snapshot = cwd.join(format!("{session_id}.json"));
        let authority = SessionAuthority::open(
            &snapshot,
            &session_id,
            WORKSPACE,
            GENERATION,
            actor(),
            "2026-08-21T12:00:00Z",
        )
        .unwrap();
        Host {
            _dir: dir,
            cwd,
            supervisor: InteractiveRuntimeSupervisor::with_authority(authority).unwrap(),
            conversation: ConversationState::new(),
            session_id,
            resume_info: None,
        }
    }

    fn request(host: &Host, id: &str) -> SessionReplacementRequest {
        SessionReplacementRequest {
            kind: SessionReplacementKind::New,
            target_session_id: id.into(),
            target_snapshot: host.cwd.join(format!("{id}.json")),
            target_conversation: ConversationState::new(),
            target_meta: None,
            resume_info: None,
        }
    }

    fn replace_host(
        host: &mut Host,
        request: SessionReplacementRequest,
    ) -> Result<SessionReplacementOutcome, SessionReplacementError> {
        replace(
            HostSessionPublication {
                supervisor: &mut host.supervisor,
                conversation: &mut host.conversation,
                displayed_session_id: &mut host.session_id,
                resume_info: &mut host.resume_info,
            },
            request,
            SessionReplacementEnvironment {
                cwd: &host.cwd,
                persist_current: false,
                workspace_identity: WORKSPACE,
                runtime_generation_id: GENERATION,
                actor: actor(),
            },
        )
    }

    fn admit(host: &mut Host) -> u64 {
        host.supervisor
            .admit_prompt(
                "prompt".into(),
                Vec::new(),
                RuntimeActor::tui(),
                ControlSurface::Tui,
                crate::operator_commands::PromptMetadata::default(),
                Some(QueueMode::UntilReady),
            )
            .unwrap()
    }

    fn wait_for_projection(supervisor: &InteractiveRuntimeSupervisor, sequence: u64) {
        for _ in 0..200 {
            if supervisor
                .projection_worker_snapshot()
                .is_some_and(|snapshot| snapshot.last_frontier_sequence == Some(sequence))
            {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("projection did not reach sequence {sequence}");
    }

    #[test]
    fn active_turn_and_queued_prompt_reject_without_mutating_original() {
        let mut active = host();
        admit(&mut active);
        active.supervisor.start_next_turn().unwrap().unwrap();
        let original = active.session_id.clone();
        let target = request(&active, "2026-08-21T12-00-01_00000002");
        assert!(matches!(
            replace_host(&mut active, target),
            Err(SessionReplacementError::Rejected(
                SessionReplacementRejection::ActiveTurn
            ))
        ));
        assert_eq!(active.session_id, original);

        let mut queued = host();
        admit(&mut queued);
        let original_authority = queued.supervisor.invocation_authority().unwrap();
        let target = request(&queued, "2026-08-21T12-00-02_00000003");
        assert!(matches!(
            replace_host(&mut queued, target),
            Err(SessionReplacementError::Rejected(
                SessionReplacementRejection::QueuedPrompts
            ))
        ));
        assert_eq!(queued.session_id, original_authority.session_id());
        assert_eq!(queued.supervisor.queue_depth(), 1);
    }

    #[test]
    fn target_corruption_and_writer_conflict_leave_original_usable() {
        let mut host = host();
        let original = host.session_id.clone();
        let corrupt_id = "2026-08-21T12-00-03_00000004";
        std::fs::write(
            host.cwd.join(format!("{corrupt_id}.authority.jsonl")),
            b"not-json\n",
        )
        .unwrap();
        let target = request(&host, corrupt_id);
        assert!(matches!(
            replace_host(&mut host, target),
            Err(SessionReplacementError::Target(_))
        ));
        assert_eq!(host.session_id, original);

        let conflict_id = "2026-08-21T12-00-04_00000005";
        let conflict_snapshot = host.cwd.join(format!("{conflict_id}.json"));
        let _writer = SessionAuthority::open(
            &conflict_snapshot,
            conflict_id,
            WORKSPACE,
            GENERATION,
            actor(),
            "2026-08-21T12:00:00Z",
        )
        .unwrap();
        let target = request(&host, conflict_id);
        assert!(matches!(
            replace_host(&mut host, target),
            Err(SessionReplacementError::Target(_))
        ));
        assert_eq!(host.session_id, original);
        assert_eq!(host.supervisor.queue_depth(), 0);
        admit(&mut host);
        assert_eq!(host.supervisor.queue_depth(), 1);
    }

    #[test]
    fn target_attachment_loss_fails_replay_and_preserves_original() {
        let mut host = host();
        let original = host.session_id.clone();
        let target_id = "2026-08-21T12-00-09_0000000a";
        let target_snapshot = host.cwd.join(format!("{target_id}.json"));
        let attachment = host.cwd.join("attachment.txt");
        std::fs::write(&attachment, b"required attachment").unwrap();
        let authority = SessionAuthority::open(
            &target_snapshot,
            target_id,
            WORKSPACE,
            GENERATION,
            actor(),
            "2026-08-21T12:00:00Z",
        )
        .unwrap();
        let mut target_supervisor =
            InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        target_supervisor
            .admit_prompt(
                "target".into(),
                vec![attachment],
                RuntimeActor::tui(),
                ControlSurface::Tui,
                crate::operator_commands::PromptMetadata::default(),
                Some(QueueMode::UntilReady),
            )
            .unwrap();
        target_supervisor.start_next_turn().unwrap().unwrap();
        let identity = target_supervisor.current_identity().unwrap();
        target_supervisor
            .submit_loop_terminal_intent(crate::runtime_turn::LoopTerminalIntent {
                identity,
                outcome: crate::runtime_turn::RuntimeTurnOutcome::Completed,
                reason_code: "test_completed".into(),
            })
            .unwrap();
        drop(target_supervisor);
        std::fs::remove_dir_all(host.cwd.join(format!("{target_id}.authority.attachments")))
            .unwrap();

        let target = request(&host, target_id);
        assert!(matches!(
            replace_host(&mut host, target),
            Err(SessionReplacementError::Target(_))
        ));
        assert_eq!(host.session_id, original);
        assert_eq!(
            host.supervisor.invocation_authority().unwrap().session_id(),
            original
        );
    }

    #[test]
    fn replacement_and_prompt_admission_serialize_with_exactly_one_winner() {
        let mut replacement_first = host();
        let target_id = "2026-08-21T12-00-05_00000006";
        let target = request(&replacement_first, target_id);
        replace_host(&mut replacement_first, target).unwrap();
        admit(&mut replacement_first);
        assert_eq!(
            replacement_first
                .supervisor
                .invocation_authority()
                .unwrap()
                .session_id(),
            target_id
        );

        let mut prompt_first = host();
        admit(&mut prompt_first);
        let original = prompt_first.session_id.clone();
        let target = request(&prompt_first, "2026-08-21T12-00-06_00000007");
        assert!(matches!(
            replace_host(&mut prompt_first, target),
            Err(SessionReplacementError::Rejected(
                SessionReplacementRejection::QueuedPrompts
            ))
        ));
        assert_eq!(prompt_first.session_id, original);
    }

    #[test]
    fn successful_replacement_moves_authority_projection_and_next_prompt_together() {
        let mut host = host();
        host.conversation.push_user("old lineage".into());
        let old_generation = host.supervisor.host_session_generation();
        let target_id = "2026-08-21T12-00-07_00000008";
        let target = request(&host, target_id);
        let outcome = replace_host(&mut host, target).unwrap();
        assert_eq!(outcome.session_id, target_id);
        assert_eq!(outcome.host_generation, old_generation + 1);
        assert_eq!(outcome.projection.session_id, target_id);
        assert_eq!(host.session_id, target_id);
        assert_eq!(host.conversation.last_user_prompt(), "");
        assert_eq!(
            host.supervisor.projection_binding().unwrap(),
            &outcome.projection
        );

        admit(&mut host);
        let authority = host.supervisor.invocation_authority().unwrap();
        assert_eq!(authority.session_id(), target_id);
        assert_eq!(authority.state().queued_prompts.len(), 1);
    }

    #[test]
    fn replacement_drains_old_worker_and_fences_old_authority_from_new_root() {
        let mut host = host();
        let old_id = host.session_id.clone();
        let old_authority = host.supervisor.invocation_authority().unwrap();
        let old_sequence = old_authority.state().last_sequence;
        wait_for_projection(&host.supervisor, old_sequence);
        let old_root = host.cwd.join(format!("{old_id}.projections"));

        let target_id = "2026-08-21T12-00-17_00000018";
        let target = request(&host, target_id);
        replace_host(&mut host, target).unwrap();
        let old_cursor = std::fs::read(
            old_root
                .join("session.frontend-snapshot")
                .join("cursor.json"),
        )
        .unwrap();
        old_authority
            .admit_prompt(
                Uuid::new_v4(),
                "2026-08-22T00:00:01Z",
                crate::session_authority::PromptAdmitted {
                    submission_id: Uuid::new_v4(),
                    prompt_id: Uuid::new_v4(),
                    principal: "test".into(),
                    ingress: "test".into(),
                    queue_mode: crate::session_authority::QueueMode::UntilReady,
                    content: crate::session_authority::PromptContent {
                        text: "late old-session append".into(),
                        attachments: Vec::new(),
                    },
                    metadata: serde_json::json!({}),
                },
            )
            .unwrap();
        thread::sleep(Duration::from_millis(75));
        assert_eq!(
            std::fs::read(
                old_root
                    .join("session.frontend-snapshot")
                    .join("cursor.json")
            )
            .unwrap(),
            old_cursor
        );
        let target_sequence = host
            .supervisor
            .invocation_authority()
            .unwrap()
            .state()
            .last_sequence;
        wait_for_projection(&host.supervisor, target_sequence);
        assert!(host.cwd.join(format!("{target_id}.projections")).exists());
    }

    #[test]
    fn resume_new_and_context_clear_share_the_same_publication_contract() {
        for (index, kind) in [
            SessionReplacementKind::Resume,
            SessionReplacementKind::New,
            SessionReplacementKind::ContextClear,
        ]
        .into_iter()
        .enumerate()
        {
            let mut host = host();
            let target_id = format!("2026-08-21T12-01-0{index}_0000001{index}");
            let mut target = request(&host, &target_id);
            target.kind = kind;
            let outcome = replace_host(&mut host, target).unwrap();
            assert_eq!(outcome.kind, kind);
            assert_eq!(host.session_id, target_id);
            assert_eq!(
                host.supervisor.projection_binding().unwrap().session_id,
                target_id
            );
        }
    }

    #[test]
    fn target_open_turn_recovers_before_publication() {
        let mut host = host();
        let target_id = "2026-08-21T12-00-08_00000009";
        let target_snapshot = host.cwd.join(format!("{target_id}.json"));
        let authority = SessionAuthority::open(
            &target_snapshot,
            target_id,
            WORKSPACE,
            GENERATION,
            actor(),
            "2026-08-21T12:00:00Z",
        )
        .unwrap();
        let mut target_supervisor =
            InteractiveRuntimeSupervisor::with_authority(authority).unwrap();
        target_supervisor
            .admit_prompt(
                "crashed target".into(),
                Vec::new(),
                RuntimeActor::tui(),
                ControlSurface::Tui,
                crate::operator_commands::PromptMetadata::default(),
                Some(QueueMode::UntilReady),
            )
            .unwrap();
        target_supervisor.start_next_turn().unwrap().unwrap();
        drop(target_supervisor);

        let recovered = SessionAuthority::open(
            &target_snapshot,
            target_id,
            WORKSPACE,
            GENERATION,
            actor(),
            "2026-08-21T12:00:01Z",
        )
        .unwrap();
        let recovered = SessionAuthorityHandle::new(recovered);
        crate::session_host_storage::save_full_spine(
            &crate::session_host_storage::SessionStorageBinding::from_authority(
                &target_snapshot,
                target_id,
                Some(&recovered),
                &host.cwd,
            ),
            &ConversationState::new(),
            None,
        )
        .unwrap();
        drop(recovered);

        let target = request(&host, target_id);
        replace_host(&mut host, target).unwrap();
        let state = host.supervisor.invocation_authority().unwrap().state();
        assert!(state.active_turn.is_none());
        assert!(state.active_step.is_none());
        assert_eq!(host.session_id, target_id);
    }
}
