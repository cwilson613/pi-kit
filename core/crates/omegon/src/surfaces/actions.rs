//! Owner-neutral action identity and transport-narrowing vocabulary.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlRole {
    Read,
    Edit,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlIngress {
    Slash,
    Cli,
    Ipc,
    WebDaemon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalAction {
    ContextView,
    ContextCompact,
    ContextClear,
    ContextRequest,
    ContextSetClass,
    SkillsView,
    SkillsGet,
    SkillsCreate,
    SkillsUpdate,
    SkillsDelete,
    SkillsInstall,
    PromptsList,
    PromptsGet,
    PromptsCreate,
    PromptsUpdate,
    PromptsDelete,
    PromptsPreview,
    PromptsSubmit,
    ModelView,
    ModelList,
    ModelSetSameProvider,
    ProviderSwitch,
    DispatcherSwitch,
    ThinkingSet,
    StatusView,
    SessionStatsView,
    TreeView,
    NoteAdd,
    NotesView,
    NotesClear,
    CheckinView,
    SessionNew,
    SessionList,
    TurnCancel,
    RuntimeShutdown,
    RuntimeReload,
    RuntimeRestart,
    UpdateInstall,
    PromptSubmit,
    AuthStatus,
    AuthLogin,
    AuthLogout,
    AuthUnlock,
    SecretsView,
    SecretsSet,
    SecretsGet,
    SecretsDelete,
    VariablesView,
    VariablesSet,
    VariablesGet,
    VariablesDelete,
    PluginView,
    PluginInstall,
    PluginRemove,
    PluginUpdate,
    CleaveView,
    CleaveCancelChild,
    DelegateStatus,
    AgentsStatus,
    MaxTurnsSet,
    ProfileView,
    ProfileExport,
    ProfileEdit,
    ProfileApply,
    PersonaList,
    PersonaSwitch,
    RuntimeModeSet,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedAction {
    pub ingress: ControlIngress,
    pub action: CanonicalAction,
    pub role: ControlRole,
    pub remote_safe: bool,
}
