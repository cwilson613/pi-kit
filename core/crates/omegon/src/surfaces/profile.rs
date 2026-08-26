use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileDriftProjection {
    pub profile_label: String,
    pub source: ProfileSourceProjection,
    pub dirty: bool,
    pub changed_count: usize,
    pub rows: Vec<ProfileDriftRow>,
    pub actions: Vec<ProfileDriftAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSourceProjection {
    pub kind: ProfileSourceKind,
    pub path: Option<PathBuf>,
    pub label: String,
    pub display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSourceKind {
    Project,
    User,
    BuiltInDefault,
}

impl std::fmt::Display for ProfileSourceProjection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.display)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileDriftRow {
    pub key: &'static str,
    pub label: &'static str,
    pub profile_value: String,
    pub runtime_value: String,
    pub persistence: PersistenceSemantics,
    pub severity: DriftSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceSemantics {
    SavedDefault,
    LiveOnly,
}

impl PersistenceSemantics {
    pub fn label(self) -> &'static str {
        match self {
            Self::SavedDefault => "saved default",
            Self::LiveOnly => "live only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftSeverity {
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileDriftAction {
    View,
    Save,
    Apply,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{ContextClass, Profile, ProfileSource, Settings, ThinkingLevel};

    fn source() -> ProfileSource {
        ProfileSource::BuiltInDefault
    }

    #[test]
    fn clean_profile_runtime_pair_has_no_drift() {
        let profile = Profile {
            thinking_level: Some("high".into()),
            requested_context_class: Some("massive".into()),
            ..Profile::default()
        };
        let mut settings = Settings {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };
        settings.set_requested_context_class(ContextClass::Massive);

        let projection =
            ProfileDriftProjection::from_profile_and_settings(&profile, source(), &settings);

        assert!(!projection.dirty);
        assert_eq!(projection.changed_count, 0);
        assert!(projection.rows.is_empty());
        assert_eq!(projection.actions, vec![ProfileDriftAction::View]);
    }

    #[test]
    fn thinking_drift_yields_stable_row() {
        let profile = Profile {
            thinking_level: Some("medium".into()),
            ..Profile::default()
        };
        let settings = Settings {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };

        let projection =
            ProfileDriftProjection::from_profile_and_settings(&profile, source(), &settings);

        assert!(projection.dirty);
        assert_eq!(projection.changed_count, 1);
        assert_eq!(projection.rows[0].key, "thinking");
        assert_eq!(projection.rows[0].profile_value, "medium");
        assert_eq!(projection.rows[0].runtime_value, "high");
        assert_eq!(
            projection.rows[0].persistence,
            PersistenceSemantics::LiveOnly
        );
    }

    #[test]
    fn requested_context_class_drift_yields_stable_row() {
        let profile = Profile {
            requested_context_class: Some("extended".into()),
            ..Profile::default()
        };
        let mut settings = Settings::default();
        settings.set_requested_context_class(ContextClass::Massive);

        let projection =
            ProfileDriftProjection::from_profile_and_settings(&profile, source(), &settings);

        assert!(projection.dirty);
        assert_eq!(projection.changed_count, 1);
        assert_eq!(projection.rows[0].key, "requestedContextClass");
        assert_eq!(projection.rows[0].label, "Context class");
        assert_eq!(projection.rows[0].profile_value, "extended");
        assert_eq!(projection.rows[0].runtime_value, "massive");
    }

    #[test]
    fn multiple_drift_rows_keep_stable_order() {
        let profile = Profile {
            thinking_level: Some("medium".into()),
            requested_context_class: Some("extended".into()),
            ..Profile::default()
        };
        let mut settings = Settings {
            thinking: ThinkingLevel::High,
            ..Default::default()
        };
        settings.set_requested_context_class(ContextClass::Massive);

        let projection =
            ProfileDriftProjection::from_profile_and_settings(&profile, source(), &settings);

        assert_eq!(projection.changed_count, 2);
        assert_eq!(projection.rows[0].key, "thinking");
        assert_eq!(projection.rows[1].key, "requestedContextClass");
        assert_eq!(
            projection.actions,
            vec![
                ProfileDriftAction::View,
                ProfileDriftAction::Save,
                ProfileDriftAction::Apply,
            ]
        );
    }
}
