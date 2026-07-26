//! Progressive skill disclosure.
//!
//! Skills carry rich activation metadata (`activation`, `project_signals`,
//! `triggers`) that historically had no runtime consumer: a host either inlined
//! every `SKILL.md` body or surfaced none. This module turns that metadata into
//! a tiered projection so identity can stay resident while bodies defer.
//!
//! The resident tier is load-bearing. Deferral is only safe because a skill's
//! name and description remain visible; a skill the model cannot see is a skill
//! it cannot reach for.

use std::path::Path;

use crate::{SkillActivation, SkillManifest};

/// Minimum description length. This is a *placeholder* gate, not a quality
/// gate: measured against the bundled library the shortest legitimate
/// description is 43 characters, so this floor rejects scaffolding leftovers
/// without penalising terse-but-clear keys.
pub const MIN_DESCRIPTION_CHARS: usize = 24;

/// Lowercase tokens that indicate an unfinished description.
const PLACEHOLDER_TOKENS: &[&str] = &["todo", "tbd", "fixme", "xxx", "description", "wip"];

/// Which disclosure tier a skill occupies for the current context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisclosureTier {
    /// Name, description, and activation only. Always present.
    Resident,
    /// Full `SKILL.md` body admitted into context.
    Triggered,
}

impl DisclosureTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resident => "resident",
            Self::Triggered => "triggered",
        }
    }

    pub fn admits_body(self) -> bool {
        matches!(self, Self::Triggered)
    }
}

/// Why a skill's body was or was not admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionReason {
    /// `activation: always`.
    Unconditional,
    /// A declared `project_signal` exists in the workspace.
    SignalMatched,
    /// A declared `trigger` matched the operator prompt.
    TriggerMatched,
    /// Declared signals are absent from the workspace.
    SignalAbsent,
    /// No declared trigger matched.
    TriggerAbsent,
    /// Activation metadata is missing or unrecognised.
    ActivationUndeclared,
}

impl AdmissionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unconditional => "unconditional",
            Self::SignalMatched => "signal_matched",
            Self::TriggerMatched => "trigger_matched",
            Self::SignalAbsent => "signal_absent",
            Self::TriggerAbsent => "trigger_absent",
            Self::ActivationUndeclared => "activation_undeclared",
        }
    }
}

/// A finding from retrieval-key lint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalKeyFinding {
    Missing,
    TooShort { len: usize },
    Placeholder { token: String },
}

impl RetrievalKeyFinding {
    pub fn message(&self) -> String {
        match self {
            Self::Missing => {
                "description is empty; it is the only retrieval key for a deferred body".into()
            }
            Self::TooShort { len } => format!(
                "description is {len} chars; needs at least {MIN_DESCRIPTION_CHARS} to serve as a retrieval key"
            ),
            Self::Placeholder { token } => {
                format!("description contains placeholder text '{token}'")
            }
        }
    }
}

/// One resident-tier entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDisclosureEntry {
    pub name: String,
    pub description: String,
    pub activation: Option<SkillActivation>,
    pub signals_matched: bool,
    pub tier: DisclosureTier,
    pub reason: AdmissionReason,
    pub retrieval_key_finding: Option<RetrievalKeyFinding>,
}

impl SkillDisclosureEntry {
    pub fn admits_body(&self) -> bool {
        self.tier.admits_body()
    }
}

/// The full projection over an installed skill set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillDisclosure {
    pub entries: Vec<SkillDisclosureEntry>,
}

impl SkillDisclosure {
    pub fn entry(&self, name: &str) -> Option<&SkillDisclosureEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Names whose bodies are admitted, in projection order.
    pub fn admitted(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.admits_body())
            .map(|e| e.name.as_str())
            .collect()
    }

    /// Entries carrying a retrieval-key finding.
    pub fn retrieval_key_findings(&self) -> Vec<(&str, &RetrievalKeyFinding)> {
        self.entries
            .iter()
            .filter_map(|e| {
                e.retrieval_key_finding
                    .as_ref()
                    .map(|f| (e.name.as_str(), f))
            })
            .collect()
    }
}

/// Lint a skill's declared activation value.
///
/// [`SkillActivation::parse`] returns `None` for anything unrecognised, and an
/// unparseable activation is never admitted. A typo like `project-detected`
/// therefore makes a skill permanently invisible with no diagnostic — the same
/// silent-failure class as an unmatchable project signal.
pub fn lint_activation(activation: Option<&str>) -> Option<String> {
    let raw = activation?;
    if SkillActivation::parse(raw).is_some() {
        return None;
    }
    Some(format!(
        "activation `{raw}` is not recognised, so this skill can never be \
         admitted; expected one of: always, intent_detected, project_detected, \
         domain_detected, lifecycle_gated"
    ))
}

/// Lint a skill's declared project signals.
///
/// A signal rejected by [`crate::validate_project_signal`] never matches
/// anything. That failure is silent: the skill simply stays resident forever
/// and the author has no reason to suspect it. The bundled `style` skill
/// shipped two `dir/*.ext` signals that had never matched before this lint.
pub fn lint_project_signals(signals: &[String]) -> Vec<String> {
    signals
        .iter()
        .filter_map(|signal| {
            crate::validate_project_signal(signal)
                .err()
                .map(|err| format!("{err}; use `dir/**/*.ext` for directory-scoped matches"))
        })
        .collect()
}

/// Validate a description as a retrieval key.
pub fn lint_retrieval_key(description: &str) -> Option<RetrievalKeyFinding> {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        return Some(RetrievalKeyFinding::Missing);
    }
    let lower = trimmed.to_lowercase();
    for token in PLACEHOLDER_TOKENS {
        // Whole-word match so "described" does not trip on "description".
        if lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == *token)
        {
            return Some(RetrievalKeyFinding::Placeholder {
                token: (*token).to_string(),
            });
        }
    }
    let len = trimmed.chars().count();
    if len < MIN_DESCRIPTION_CHARS {
        return Some(RetrievalKeyFinding::TooShort { len });
    }
    None
}

/// Does any declared signal exist in the workspace?
///
/// Delegates to the crate's validated project-signal matcher so disclosure and
/// skill suggestion share one definition of workspace evidence. That matcher
/// rejects traversal shapes (`..`, absolute, backslash, NUL), supports literal,
/// root-glob, and one-segment recursive globs, and skips vendor/build
/// directories so a dependency tree cannot activate a skill.
///
/// A signal that fails validation never matches: an invalid declaration is an
/// authoring error, not licence to admit a body.
pub fn signals_match(root: &Path, signals: &[String]) -> bool {
    signals
        .iter()
        .any(|signal| matches!(crate::match_project_signal(root, signal), Ok(Some(_))))
}

/// Does the prompt name a declared trigger?
///
/// Whole-word matching. Substring containment admits bodies on accidents —
/// `"build"` contains `"ui"`, `"reiterate"` contains `"iterate"` — which
/// silently defeats the point of withholding them.
fn trigger_matches(triggers: &[String], prompt: Option<&str>) -> bool {
    let Some(prompt) = prompt else {
        return false;
    };
    let lower = prompt.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    triggers.iter().any(|trigger| {
        let trigger = trigger.trim().to_lowercase();
        if trigger.is_empty() {
            return false;
        }
        // Multi-word triggers ("landing page") match as an ordered word run.
        let needle: Vec<&str> = trigger
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        match needle.len() {
            0 => false,
            1 => words.contains(&needle[0]),
            n => words.windows(n).any(|w| w == needle.as_slice()),
        }
    })
}

/// Decide the tier for one skill.
///
/// Admission is evidence-backed: unconditional `always`, a matched workspace
/// signal, or a matched intent trigger. Undeclared activation is never
/// admitted — an unlabelled skill is an authoring gap, not licence to infer
/// relevance from its name.
pub fn admit(
    manifest: &SkillManifest,
    signals_matched: bool,
    prompt: Option<&str>,
) -> (DisclosureTier, AdmissionReason) {
    let activation = manifest
        .activation
        .as_deref()
        .and_then(SkillActivation::parse);

    match activation {
        None => (
            DisclosureTier::Resident,
            AdmissionReason::ActivationUndeclared,
        ),
        Some(SkillActivation::Always) => {
            (DisclosureTier::Triggered, AdmissionReason::Unconditional)
        }
        Some(SkillActivation::IntentDetected) => {
            if trigger_matches(&manifest.triggers, prompt) {
                (DisclosureTier::Triggered, AdmissionReason::TriggerMatched)
            } else {
                (DisclosureTier::Resident, AdmissionReason::TriggerAbsent)
            }
        }
        Some(
            SkillActivation::ProjectDetected
            | SkillActivation::DomainDetected
            | SkillActivation::LifecycleGated,
        ) => {
            if signals_matched {
                (DisclosureTier::Triggered, AdmissionReason::SignalMatched)
            } else if trigger_matches(&manifest.triggers, prompt) {
                // Explicit intent is evidence too. A signal-gated skill whose
                // marker files are absent is still the right body to load when
                // the operator names its trigger — otherwise "write an OpenSpec
                // proposal" in a repo without `openspec/` can never bootstrap.
                (DisclosureTier::Triggered, AdmissionReason::TriggerMatched)
            } else {
                (DisclosureTier::Resident, AdmissionReason::SignalAbsent)
            }
        }
    }
}

/// Build a disclosure entry for one skill.
pub fn disclose(
    manifest: &SkillManifest,
    root: &Path,
    prompt: Option<&str>,
) -> SkillDisclosureEntry {
    let signals_matched = signals_match(root, &manifest.project_signals);
    let (tier, reason) = admit(manifest, signals_matched, prompt);
    SkillDisclosureEntry {
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        activation: manifest
            .activation
            .as_deref()
            .and_then(SkillActivation::parse),
        signals_matched,
        tier,
        reason,
        retrieval_key_finding: lint_retrieval_key(&manifest.description),
    }
}

/// Build the projection over an installed skill set.
pub fn build_disclosure<'a, I>(manifests: I, root: &Path, prompt: Option<&str>) -> SkillDisclosure
where
    I: IntoIterator<Item = &'a SkillManifest>,
{
    SkillDisclosure {
        entries: manifests
            .into_iter()
            .map(|m| disclose(m, root, prompt))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str, activation: Option<&str>) -> SkillManifest {
        SkillManifest {
            name: name.into(),
            description: format!("Conventions and patterns for {name} development work"),
            activation: activation.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn triggers_match_whole_words_not_substrings() {
        let mut m = manifest("flynt-design", Some("intent_detected"));
        m.triggers = vec!["ui".into(), "page".into(), "form".into()];

        // Regression: substring containment admitted this body on "build"
        // ("b-UI-ld"), "guide", "require", and "quick" — routine words in this
        // repo. Withholding a body only pays if it stays withheld.
        for prompt in [
            "build the release binary",
            "fix the user guide",
            "require approval",
            "quick audit",
            "reformat the transform",
        ] {
            let (tier, _) = admit(&m, false, Some(prompt));
            assert_eq!(
                tier,
                DisclosureTier::Resident,
                "{prompt:?} must not admit on a substring accident"
            );
        }

        let (tier, reason) = admit(&m, false, Some("redesign the ui layout"));
        assert_eq!(tier, DisclosureTier::Triggered);
        assert_eq!(reason, AdmissionReason::TriggerMatched);
    }

    #[test]
    fn multi_word_triggers_match_as_an_ordered_run() {
        let mut m = manifest("flynt-design", Some("intent_detected"));
        m.triggers = vec!["landing page".into()];

        let (hit, _) = admit(&m, false, Some("build me a landing page"));
        assert_eq!(hit, DisclosureTier::Triggered);

        // Both words present but not adjacent/ordered is not the trigger.
        let (miss, _) = admit(&m, false, Some("page through the landing data"));
        assert_eq!(miss, DisclosureTier::Resident);
    }

    #[test]
    fn explicit_intent_admits_a_signal_gated_skill_without_its_markers() {
        let mut m = manifest("openspec", Some("lifecycle_gated"));
        m.project_signals = vec!["openspec/changes".into()];
        m.triggers = vec!["openspec".into()];
        let dir = tmpdir();

        // Markers absent: silence without intent.
        let (quiet, reason) = admit(&m, false, None);
        assert_eq!(quiet, DisclosureTier::Resident);
        assert_eq!(reason, AdmissionReason::SignalAbsent);

        // Operator names it: intent is evidence, so the body loads even though
        // the workspace has no openspec/ directory yet to bootstrap from.
        let (loud, reason) = admit(&m, false, Some("write an openspec proposal"));
        assert_eq!(loud, DisclosureTier::Triggered);
        assert_eq!(reason, AdmissionReason::TriggerMatched);
        assert!(!signals_match(dir.path(), &m.project_signals));
    }

    #[test]
    fn signal_matching_refuses_traversal_escapes() {
        let dir = tmpdir();
        let root = dir.path().join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        // A real file outside the workspace root.
        std::fs::write(dir.path().join("outside.txt"), "x").unwrap();

        for escape in [
            "../outside.txt",
            "../../etc/passwd",
            "/etc/passwd",
            "sub/../../outside.txt",
        ] {
            assert!(
                !signals_match(&root, &[escape.to_string()]),
                "{escape:?} must not resolve outside the workspace root"
            );
        }
    }

    #[test]
    fn signal_matching_ignores_vendor_directories() {
        let dir = tmpdir();
        let root = dir.path();
        let vendored = root.join("node_modules").join("pkg");
        std::fs::create_dir_all(&vendored).unwrap();
        std::fs::write(vendored.join("tsconfig.json"), "{}").unwrap();

        // A dependency's config must not activate the TypeScript body.
        assert!(!signals_match(root, &["**/tsconfig.json".to_string()]));

        std::fs::write(root.join("tsconfig.json"), "{}").unwrap();
        assert!(signals_match(root, &["tsconfig.json".to_string()]));
    }

    #[test]
    fn recursive_globs_match_through_the_shared_validated_matcher() {
        let dir = tmpdir();
        let root = dir.path();
        let nested = root.join("docs").join("adr");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("0001.md"), "#").unwrap();

        // The previous local matcher silently never matched `**` shapes.
        assert!(signals_match(root, &["docs/**/*.md".to_string()]));
    }

    #[test]
    fn always_activation_is_admitted_unconditionally() {
        let m = manifest("security", Some("always"));
        let (tier, reason) = admit(&m, false, None);
        assert_eq!(tier, DisclosureTier::Triggered);
        assert_eq!(reason, AdmissionReason::Unconditional);
    }

    #[test]
    fn signal_backed_activations_follow_workspace_evidence() {
        for activation in ["project_detected", "domain_detected", "lifecycle_gated"] {
            let m = manifest("typescript", Some(activation));

            let (tier, reason) = admit(&m, true, None);
            assert_eq!(tier, DisclosureTier::Triggered, "{activation}");
            assert_eq!(reason, AdmissionReason::SignalMatched, "{activation}");

            let (tier, reason) = admit(&m, false, None);
            assert_eq!(tier, DisclosureTier::Resident, "{activation}");
            assert_eq!(reason, AdmissionReason::SignalAbsent, "{activation}");
        }
    }

    #[test]
    fn intent_activation_requires_a_matching_trigger() {
        let mut m = manifest("iterator", Some("intent_detected"));
        m.triggers = vec!["visual iteration".into(), "iterate".into()];

        let (tier, reason) = admit(&m, false, Some("please iterate on the UI"));
        assert_eq!(tier, DisclosureTier::Triggered);
        assert_eq!(reason, AdmissionReason::TriggerMatched);

        let (tier, reason) = admit(&m, false, Some("refactor the parser"));
        assert_eq!(tier, DisclosureTier::Resident);
        assert_eq!(reason, AdmissionReason::TriggerAbsent);

        let (tier, _) = admit(&m, false, None);
        assert_eq!(tier, DisclosureTier::Resident, "no prompt cannot trigger");
    }

    #[test]
    fn undeclared_activation_is_never_admitted() {
        for activation in [None, Some("sometimes"), Some("")] {
            let m = manifest("mystery", activation);
            let (tier, reason) = admit(&m, true, Some("mystery"));
            assert_eq!(
                tier,
                DisclosureTier::Resident,
                "activation {activation:?} must not admit a body"
            );
            assert_eq!(reason, AdmissionReason::ActivationUndeclared);
        }
    }

    #[test]
    fn admission_never_infers_relevance_from_the_skill_name() {
        // `oci` declares a Containerfile signal. A workspace without one must
        // not admit the body merely because the operator said the word.
        let mut m = manifest("oci", Some("domain_detected"));
        m.project_signals = vec!["Containerfile".into()];
        let dir = tmpdir();

        let entry = disclose(&m, dir.path(), Some("let's talk about oci registries"));

        assert_eq!(entry.tier, DisclosureTier::Resident);
        assert_eq!(entry.reason, AdmissionReason::SignalAbsent);
        assert!(!entry.signals_matched);
    }

    #[test]
    fn unmatched_skills_remain_resident_and_discoverable() {
        let mut m = manifest("oci", Some("domain_detected"));
        m.project_signals = vec!["Containerfile".into()];
        let dir = tmpdir();

        let disclosure = build_disclosure([&m], dir.path(), None);
        let entry = disclosure.entry("oci").expect("resident entry");

        assert_eq!(entry.name, "oci");
        assert!(!entry.description.is_empty(), "identity stays visible");
        assert_eq!(entry.activation, Some(SkillActivation::DomainDetected));
        assert!(!entry.admits_body());
        assert!(disclosure.admitted().is_empty());
    }

    #[test]
    fn literal_signals_match_by_existence_only() {
        let dir = tmpdir();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        assert!(signals_match(dir.path(), &["Cargo.toml".into()]));
        assert!(!signals_match(dir.path(), &["tsconfig.json".into()]));
        assert!(!signals_match(dir.path(), &[]));
    }

    /// `skills_get` resolves a skill by *directory* name, while the disclosure
    /// index advertises the *manifest* name. If those disagree, the model is
    /// told to retrieve a skill under a key that cannot be resolved — the body
    /// is withheld and unreachable, which is silent capability loss.
    ///
    /// This invariant is what makes withholding safe, so it is enforced rather
    /// than assumed.
    #[test]
    fn bundled_skill_directory_names_match_their_manifest_names() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .join("skills");
        let mut checked = 0usize;

        for entry in std::fs::read_dir(&root).unwrap() {
            let dir = entry.unwrap().path();
            let Ok(raw) = std::fs::read_to_string(dir.join("SKILL.md")) else {
                continue;
            };
            let (manifest, _body) = crate::parse_skill_file(&raw);
            let dir_name = dir.file_name().unwrap().to_string_lossy();
            assert_eq!(
                manifest.name, dir_name,
                "skill in directory `{dir_name}` declares name `{}`; \
                 the disclosure index would advertise a key skills_get cannot resolve",
                manifest.name
            );
            checked += 1;
        }

        assert!(checked >= 8, "expected bundled skills, found {checked}");
    }

    #[test]
    fn every_bundled_skill_declares_matchable_signals() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .join("skills");
        let mut checked = 0usize;

        for entry in std::fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path().join("SKILL.md");
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (manifest, _) = crate::parse_skill_file(&raw);
            let findings = lint_project_signals(&manifest.project_signals);
            assert!(
                findings.is_empty(),
                "{}: declares signals that can never match: {findings:?}",
                manifest.name
            );
            assert!(
                lint_activation(manifest.activation.as_deref()).is_none(),
                "{}: {}",
                manifest.name,
                lint_activation(manifest.activation.as_deref()).unwrap()
            );
            checked += 1;
        }

        assert!(checked >= 8, "expected bundled skills, found {checked}");
    }

    #[test]
    fn unrecognised_activation_is_linted_not_silently_ignored() {
        assert!(lint_activation(None).is_none());
        assert!(lint_activation(Some("always")).is_none());
        assert!(lint_activation(Some("project_detected")).is_none());

        let finding = lint_activation(Some("project-detected")).expect("hyphen form must lint");
        assert!(finding.contains("project-detected"));
        assert!(finding.contains("never be admitted"));
    }

    #[test]
    fn invalid_signal_shapes_are_reported() {
        let findings = lint_project_signals(&[
            "drawings/*.excalidraw".into(),
            "../escape".into(),
            "Cargo.toml".into(),
            "*.rs".into(),
            "docs/**/*.md".into(),
        ]);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings[0].contains("drawings/*.excalidraw"));
        assert!(findings[0].contains("dir/**/*.ext"));
    }

    #[test]
    fn directory_scoped_globs_are_invalid_and_never_match() {
        let dir = tmpdir();
        std::fs::create_dir(dir.path().join("drawings")).unwrap();
        std::fs::write(dir.path().join("drawings/arch.excalidraw"), "").unwrap();

        // `dir/*.ext` is rejected by validate_project_signal: a root glob must
        // not contain '/'. The file exists and the author's intent is obvious,
        // but the declaration is invalid, so it never matches. Silent
        // non-matching is exactly why lint_project_signals exists.
        assert!(!signals_match(
            dir.path(),
            &["drawings/*.excalidraw".into()]
        ));

        // The valid way to express the same intent:
        assert!(signals_match(
            dir.path(),
            &["drawings/**/*.excalidraw".into()]
        ));
    }

    #[test]
    fn root_level_globs_match() {
        let dir = tmpdir();
        std::fs::write(dir.path().join("board.board"), "").unwrap();

        assert!(signals_match(dir.path(), &["*.board".into()]));
        assert!(!signals_match(dir.path(), &["*.excalidraw".into()]));
    }

    #[test]
    fn signal_matching_does_not_recurse() {
        let dir = tmpdir();
        std::fs::create_dir_all(dir.path().join("nested/deep")).unwrap();
        std::fs::write(dir.path().join("nested/deep/Containerfile"), "").unwrap();

        assert!(
            !signals_match(dir.path(), &["Containerfile".into()]),
            "signals name marker files, not haystacks"
        );
    }

    #[test]
    fn every_bundled_skill_has_a_usable_retrieval_key() {
        // Calibration guard. The shortest description in the bundled library is
        // 43 chars, so MIN_DESCRIPTION_CHARS at 24 leaves ~2x headroom: it
        // rejects scaffolding leftovers without penalising terse real keys.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../skills")
            .canonicalize()
            .expect("bundled skills directory");

        let mut checked = 0;
        for entry in std::fs::read_dir(&root).expect("read skills dir").flatten() {
            let skill_file = entry.path().join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&skill_file).expect("read SKILL.md");
            let (manifest, _body) = crate::parse_skill_file(&content);
            if let Some(finding) = lint_retrieval_key(&manifest.description) {
                panic!(
                    "bundled skill '{}' has an unusable retrieval key: {}",
                    manifest.name,
                    finding.message()
                );
            }
            checked += 1;
        }

        assert!(
            checked >= 10,
            "expected the bundled library to be linted, only saw {checked} skills"
        );
    }

    #[test]
    fn retrieval_key_lint_rejects_unusable_descriptions() {
        assert_eq!(lint_retrieval_key(""), Some(RetrievalKeyFinding::Missing));
        assert_eq!(
            lint_retrieval_key("   "),
            Some(RetrievalKeyFinding::Missing)
        );
        assert_eq!(
            lint_retrieval_key("TODO"),
            Some(RetrievalKeyFinding::Placeholder {
                token: "todo".into()
            })
        );
        assert_eq!(
            lint_retrieval_key("skill description here"),
            Some(RetrievalKeyFinding::Placeholder {
                token: "description".into()
            })
        );
        assert!(matches!(
            lint_retrieval_key("short one"),
            Some(RetrievalKeyFinding::TooShort { .. })
        ));
    }

    #[test]
    fn retrieval_key_lint_accepts_real_descriptions() {
        // Shortest description in the bundled library, verbatim.
        assert_eq!(
            lint_retrieval_key("Conventions for TypeScript code and tooling"),
            None
        );
        assert_eq!(
            lint_retrieval_key("Defensive coding practices for implementation and review"),
            None
        );
        // Substring collisions must not trip the whole-word placeholder check.
        assert_eq!(
            lint_retrieval_key("Patterns described for reviewing Rust ownership models"),
            None
        );
    }
}
