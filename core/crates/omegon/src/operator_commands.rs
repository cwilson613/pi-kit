//! Surface-neutral operator interface boundary.
//!
//! Operator-facing frontends such as the native TUI, web sockets, IPC, and
//! extension ingress couple to this module rather than to coordinator/backend
//! internals. The boundary is intentionally a semantic command envelope plus a
//! Tokio channel handle: renderers may be tightly coupled to this contract, but
//! must not reach behind it to drive runtime state directly.

use crate::runtime_commands::CanonicalSlashCommand;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Producer-side handle for the operator interface boundary.
///
/// UI/front-end code should accept this alias instead of spelling a raw
/// `mpsc::Sender<OperatorCommand>` or importing backend coordinator types. That
/// keeps the dependency direction explicit: frontends emit semantic operator
/// commands; the runtime coordinator owns interpretation and side effects.
pub type OperatorCommandTx = mpsc::Sender<OperatorCommand>;

/// Boundary-owned control request type exposed to interface adapters.
///
/// Frontends and their tests should couple to this alias rather than importing
/// backend control runtime internals directly.
pub type InterfaceControlRequest = crate::control_runtime::ControlRequest;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoicePromptMetadata {
    pub event_id: String,
    pub duration_s: Option<f64>,
    pub radio_cue: Option<String>,
    pub end_of_turn: Option<bool>,
    pub close_session_requested: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromptMetadata {
    pub voice: Option<VoicePromptMetadata>,
}

#[derive(Debug, Clone)]
pub struct PromptSubmission {
    pub text: String,
    pub image_paths: Vec<std::path::PathBuf>,
    pub submitted_by: String,
    pub via: &'static str,
    pub queue_mode: PromptQueueMode,
    pub metadata: PromptMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptQueueMode {
    InterruptAfterTurn,
    #[default]
    UntilReady,
    Immediate,
}

/// Messages from operator surfaces to the agent coordinator.
#[derive(Debug)]
pub enum OperatorCommand {
    /// User submitted a prompt with optional image attachments.
    SubmitPrompt(PromptSubmission),
    /// Request cancellation of the active runtime turn.
    CancelActiveTurn {
        submitted_by: String,
        via: &'static str,
    },
    /// Execute a local shell command directly without LLM mediation.
    RunShellCommand {
        command: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Internal completion returned by a spawned operator shell execution so
    /// canonical conversation state remains single-owner.
    OperatorShellCompleted {
        observation: crate::conversation::OperatorToolObservation,
        committed: tokio::sync::oneshot::Sender<()>,
    },
    /// Temporarily hand terminal control to the operator's real shell.
    /// Carries the keyboard-enhancement flag so the handler can pop/push
    /// the Kitty protocol around the subprocess without querying the
    /// terminal again (which can fail if stdin is redirected).
    ShellHandoff { keyboard_enhancement: bool },
    /// User wants to quit (double Ctrl+C, or /exit).
    Quit,
    /// Download and verify an update, then enter the graceful restart lifecycle.
    InstallUpdate {
        info: crate::update::UpdateInfo,
        args: Vec<String>,
    },
    /// Gracefully save and shut down, then re-exec the current process.
    RestartProcess {
        binary: std::path::PathBuf,
        args: Vec<String>,
    },
    /// Show current model/provider posture.
    ModelView {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Show available models.
    ModelList {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Switch the model for the next turn.
    SetModel {
        model: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Switch model intent to a provider-neutral capability grade.
    SetModelGrade {
        grade: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Switch provider/endpoint selection intent.
    SetModelProvider {
        provider: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Switch model grade policy intent.
    SetModelPolicy {
        policy: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Clear exact model override and resume grade/provider intent routing.
    ModelUnpin {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Set the thinking level.
    SetThinking {
        level: crate::settings::ThinkingLevel,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Execute a canonical control request directly.
    ExecuteControl {
        request: crate::control_runtime::ControlRequest,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Execute an authenticated Auspex supervisor request against the live delegate feature.
    ManagedDelegateControl {
        method: String,
        payload: serde_json::Value,
        respond_to: tokio::sync::oneshot::Sender<serde_json::Value>,
    },
    /// Execute canonical slash semantics from a non-TUI caller.
    RunSlashCommand {
        name: String,
        args: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::SlashCommandResponse>>,
    },
    /// Update the session plan stored in the runtime conversation state.
    UpdatePlan {
        command: CanonicalSlashCommand,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Dispatch a bus command from a feature (name, args).
    BusCommand { name: String, args: String },
    /// Trigger manual compaction.
    Compact,
    /// Show context usage and status.
    ContextStatus {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Compress context and clear history.
    ContextCompact {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Clear context completely (fresh start).
    ContextClear {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// List saved sessions.
    ListSessions {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Start the local browser surface server used by Auspex compatibility flows.
    StartWebDashboard,
    /// Discard the current session and start fresh (saves current first).
    NewSession {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Probe and report auth/provider status.
    AuthStatus {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Voice transcription submitted by a process-local voice extension.
    VoicePrompt {
        text: String,
        metadata: VoicePromptMetadata,
    },
    /// Start provider login flow.
    AuthLogin {
        provider: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Log out a provider.
    AuthLogout {
        provider: String,
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
    /// Unlock secrets/auth backend.
    AuthUnlock {
        respond_to: Option<tokio::sync::oneshot::Sender<omegon_traits::ControlOutputResponse>>,
    },
}

/// Resolve a canonical slash command into the runtime control request envelope.
///
/// Frontend adapters should call this interface-boundary helper instead of
/// importing `control_runtime` directly. The coordinator remains the owner of
/// executing the returned request.
pub fn control_request_from_slash_command(
    command: &CanonicalSlashCommand,
) -> Option<crate::control_runtime::ControlRequest> {
    crate::control_runtime::control_request_from_slash(command)
}

/// Build the shared skills menu projection behind the operator interface boundary.
pub fn skills_menu_projection(
    entries: &[crate::skills::SkillEntry],
) -> crate::surfaces::menu::MenuProjection {
    crate::control_runtime::skills_menu_projection(entries)
}

/// Return renderer-neutral skills command help text.
pub fn skills_help_text() -> &'static str {
    crate::control_runtime::skills_help_text()
}

/// Shared cancellation slot written by operator surfaces and read by the agent loop.
pub type SharedCancel = std::sync::Arc<std::sync::Mutex<Option<CancellationToken>>>;
