//! Shared UI presentation policy and surface visibility primitives.
//!
//! Presentation level describes information density. Surface visibility
//! describes which high-level regions a renderer may allocate. Keeping these
//! concepts separate prevents dashboard visibility from becoming an accidental
//! proxy for transcript, activity, or telemetry semantics.

use serde::{Deserialize, Serialize};

/// Terminal ownership is independent of content density and runtime policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TerminalPresentation {
    Inline,
    #[default]
    Fullscreen,
}

impl TerminalPresentation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Fullscreen => "fullscreen",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "inline" => Ok(Self::Inline),
            "fullscreen" => Ok(Self::Fullscreen),
            _ => Err(format!(
                "Unknown terminal presentation: {value}; use inline or fullscreen"
            )),
        }
    }
}

/// Only recognized entry identities participate in default resolution.
pub fn ui_entry_name<'a>(marker: Option<&'a str>, argv0: &'a str) -> &'a str {
    marker
        .filter(|v| matches!(*v, "om" | "omegon"))
        .or_else(|| {
            std::path::Path::new(argv0)
                .file_name()
                .and_then(|v| v.to_str())
                .filter(|v| matches!(*v, "om" | "omegon"))
        })
        .unwrap_or("omegon")
}

pub fn resolve_ui_preferences(
    entry: &str,
    profile_terminal: Option<TerminalPresentation>,
    profile_detail: Option<UiPresentationLevel>,
    cli_terminal: Option<TerminalPresentation>,
    cli_detail: Option<UiPresentationLevel>,
) -> (TerminalPresentation, UiPresentationLevel) {
    let defaults = if entry == "om" {
        (TerminalPresentation::Inline, UiPresentationLevel::Active)
    } else {
        (TerminalPresentation::Fullscreen, UiPresentationLevel::Full)
    };
    (
        cli_terminal.or(profile_terminal).unwrap_or(defaults.0),
        cli_detail.or(profile_detail).unwrap_or(defaults.1),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiPresentationLevel {
    #[default]
    #[serde(skip_deserializing)]
    Om,
    #[serde(alias = "om", alias = "lean", alias = "slim")]
    Active,
    Full,
}

impl UiPresentationLevel {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Om => "om",
            Self::Active => "active",
            Self::Full => "full",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "om" | "lean" | "slim" => Ok(Self::Active),
            "active" => Ok(Self::Active),
            "full" => Ok(Self::Full),
            other => Err(format!("Unknown UI presentation level: {other}")),
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Om => Self::Active,
            Self::Active => Self::Full,
            Self::Full => Self::Active,
        }
    }

    pub const fn transcript_density(self) -> TranscriptDensity {
        match self {
            Self::Om | Self::Active => TranscriptDensity::Outcomes,
            Self::Full => TranscriptDensity::Evidence,
        }
    }

    pub const fn live_detail(self) -> LiveDetail {
        match self {
            Self::Om => LiveDetail::Status,
            Self::Active => LiveDetail::Workflow,
            Self::Full => LiveDetail::Diagnostic,
        }
    }

    pub const fn telemetry_density(self) -> TelemetryDensity {
        match self {
            Self::Om => TelemetryDensity::Essential,
            Self::Active => TelemetryDensity::Operational,
            Self::Full => TelemetryDensity::Diagnostic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptDensity {
    Outcomes,
    Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveDetail {
    Status,
    Workflow,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryDensity {
    Essential,
    Operational,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePreset {
    Om,
    Active,
    Full,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSurfaces {
    pub dashboard: bool,
    pub instruments: bool,
    pub footer: bool,
    pub activity: bool,
}

impl UiSurfaces {
    pub const fn om() -> Self {
        Self {
            dashboard: false,
            instruments: false,
            footer: false,
            activity: true,
        }
    }

    /// Compatibility constructor for callers not yet migrated to Om naming.
    pub const fn lean() -> Self {
        Self::om()
    }

    pub const fn active() -> Self {
        Self {
            dashboard: false,
            instruments: false,
            footer: false,
            activity: true,
        }
    }

    pub const fn full() -> Self {
        Self {
            dashboard: true,
            instruments: true,
            footer: true,
            activity: true,
        }
    }

    /// Legacy layout hint only. Presentation density must use
    /// `UiPresentationPolicy::level` instead.
    pub const fn is_compact(&self) -> bool {
        !self.dashboard
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiPresentationPolicy {
    pub level: UiPresentationLevel,
    pub surfaces: UiSurfaces,
    custom: bool,
}

impl Default for UiPresentationPolicy {
    fn default() -> Self {
        Self::om()
    }
}

impl UiPresentationPolicy {
    pub const fn om() -> Self {
        Self::named(UiPresentationLevel::Om)
    }

    pub const fn active() -> Self {
        Self::named(UiPresentationLevel::Active)
    }

    pub const fn full() -> Self {
        Self::named(UiPresentationLevel::Full)
    }

    pub const fn named(level: UiPresentationLevel) -> Self {
        let surfaces = match level {
            UiPresentationLevel::Om => UiSurfaces::om(),
            UiPresentationLevel::Active => UiSurfaces::active(),
            UiPresentationLevel::Full => UiSurfaces::full(),
        };
        Self {
            level,
            surfaces,
            custom: false,
        }
    }

    /// Compatibility bridge for old surface-only actions. Matching surface
    /// sets retain the current semantic level; unmatched sets become custom
    /// without changing their inherited density policy.
    pub const fn with_surfaces(mut self, surfaces: UiSurfaces) -> Self {
        self.surfaces = surfaces;
        self.custom = !Self::surfaces_match_level(self.level, surfaces);
        self
    }

    pub const fn set_surface(&mut self, surface: UiSurface, visible: bool) {
        match surface {
            UiSurface::Dashboard => self.surfaces.dashboard = visible,
            UiSurface::Instruments => self.surfaces.instruments = visible,
            UiSurface::Footer => self.surfaces.footer = visible,
            UiSurface::Activity => self.surfaces.activity = visible,
        }
        self.custom = !Self::surfaces_match_level(self.level, self.surfaces);
    }

    pub const fn preset(self) -> SurfacePreset {
        if self.custom {
            SurfacePreset::Custom
        } else {
            match self.level {
                UiPresentationLevel::Om => SurfacePreset::Om,
                UiPresentationLevel::Active => SurfacePreset::Active,
                UiPresentationLevel::Full => SurfacePreset::Full,
            }
        }
    }

    pub const fn preset_name(self) -> &'static str {
        match self.preset() {
            SurfacePreset::Om => "om",
            SurfacePreset::Active => "active",
            SurfacePreset::Full => "full",
            SurfacePreset::Custom => "custom",
        }
    }

    pub const fn next(self) -> Self {
        Self::named(self.level.next())
    }

    pub const fn transcript_density(self) -> TranscriptDensity {
        self.level.transcript_density()
    }

    pub const fn live_detail(self) -> LiveDetail {
        self.level.live_detail()
    }

    pub const fn telemetry_density(self) -> TelemetryDensity {
        self.level.telemetry_density()
    }

    const fn surfaces_match_level(level: UiPresentationLevel, surfaces: UiSurfaces) -> bool {
        let expected = match level {
            UiPresentationLevel::Om => UiSurfaces::om(),
            UiPresentationLevel::Active => UiSurfaces::active(),
            UiPresentationLevel::Full => UiSurfaces::full(),
        };
        surfaces.dashboard == expected.dashboard
            && surfaces.instruments == expected.instruments
            && surfaces.footer == expected.footer
            && surfaces.activity == expected.activity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSurface {
    Dashboard,
    Instruments,
    Footer,
    Activity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_density_maps_to_active() {
        for name in ["om", "lean", "slim"] {
            assert_eq!(
                UiPresentationLevel::parse(name),
                Ok(UiPresentationLevel::Active)
            );
        }
    }

    #[test]
    fn entry_defaults_resolve_independently() {
        assert_eq!(
            resolve_ui_preferences("om", None, None, None, None),
            (TerminalPresentation::Inline, UiPresentationLevel::Active)
        );
        assert_eq!(
            resolve_ui_preferences("omegon", None, None, None, None),
            (TerminalPresentation::Fullscreen, UiPresentationLevel::Full)
        );
        assert_eq!(
            resolve_ui_preferences(
                "om",
                Some(TerminalPresentation::Fullscreen),
                Some(UiPresentationLevel::Full),
                Some(TerminalPresentation::Inline),
                None
            ),
            (TerminalPresentation::Inline, UiPresentationLevel::Full)
        );
        assert_eq!(ui_entry_name(Some("garbage"), "/bin/om"), "om");
        assert_eq!(ui_entry_name(None, "cargo-test"), "omegon");
    }

    #[test]
    fn om_is_the_default_presentation() {
        let policy = UiPresentationPolicy::default();
        assert_eq!(policy.level, UiPresentationLevel::Om);
        assert_eq!(policy.preset(), SurfacePreset::Om);
        assert_eq!(policy.transcript_density(), TranscriptDensity::Outcomes);
        assert_eq!(policy.live_detail(), LiveDetail::Status);
        assert_eq!(policy.telemetry_density(), TelemetryDensity::Essential);
    }

    #[test]
    fn presentation_cycle_includes_active() {
        let policy = UiPresentationPolicy::om().next();
        assert_eq!(policy.level, UiPresentationLevel::Active);
        assert_eq!(policy.next().level, UiPresentationLevel::Full);
        assert_eq!(policy.next().next().level, UiPresentationLevel::Active);
    }

    #[test]
    fn legacy_names_parse_as_active() {
        for name in ["om", "lean", "slim"] {
            assert_eq!(
                UiPresentationLevel::parse(name),
                Ok(UiPresentationLevel::Active)
            );
        }
    }

    #[test]
    fn custom_surfaces_retain_base_density() {
        let mut policy = UiPresentationPolicy::active();
        policy.set_surface(UiSurface::Activity, false);
        assert_eq!(policy.preset(), SurfacePreset::Custom);
        assert_eq!(policy.level, UiPresentationLevel::Active);
        assert_eq!(policy.live_detail(), LiveDetail::Workflow);
        assert_eq!(policy.transcript_density(), TranscriptDensity::Outcomes);
    }

    #[test]
    fn om_and_active_share_surfaces_but_not_semantics() {
        let om = UiPresentationPolicy::om();
        let active = UiPresentationPolicy::active();
        assert_eq!(om.surfaces, active.surfaces);
        assert_ne!(om.level, active.level);
        assert_ne!(om.live_detail(), active.live_detail());
    }
}
