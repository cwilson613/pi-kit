use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::session_authority::{
    AuthorityError, AuthorityFrontierRef, CompactionAbandoned, CompactionApplied,
    CompactionContextItem, CompactionOwnerScope, CompactionPromptTemplate,
    CompactionReplacementItem, CompactionReplacementSourceKind, CompactionRequestClosed,
    CompactionRequestOutcome, CompactionRequestPrepared, CompactionResponseAttemptFailed,
    CompactionResponseAttemptFailure, CompactionRetryDisposition, CompactionRoute,
    CompactionStarted, CompactionSummaryCommitted, CompactionTrigger, CompactionUsage,
    ModelContextRole, ProjectionClass, ProviderCompletionEvidence, SessionAuthorityHandle,
    compaction_input_manifest_id,
};

const SUMMARY_PROMPT_PATH: &str = "prompts/session-compaction.md";

pub(crate) fn summary_prompt() -> anyhow::Result<(String, String, String)> {
    let pack = crate::content_pack::boot_pack();
    summary_prompt_from_pack(pack.as_deref())
}

fn summary_prompt_from_pack(
    pack: Option<&crate::content_pack::ContentPack>,
) -> anyhow::Result<(String, String, String)> {
    let pack = pack.ok_or_else(|| anyhow::anyhow!("shipped compaction prompt is unavailable"))?;
    let body = pack
        .text(SUMMARY_PROMPT_PATH)
        .map_err(|error| anyhow::anyhow!("shipped compaction prompt is unavailable: {error}"))?;
    Ok((
        format!("content-pack:{}", pack.manifest.id),
        pack.generation.clone(),
        body.to_string(),
    ))
}

#[derive(Debug, Clone)]
pub(crate) struct SessionCompaction {
    authority: SessionAuthorityHandle,
    compaction_id: Uuid,
    compaction_request_id: Uuid,
    owner_scope: CompactionOwnerScope,
    source_context_revision: u64,
    target_context_revision: u64,
    retained_items: Vec<CompactionContextItem>,
    provider_payload: String,
    prompt_owner: String,
    prompt_generation: String,
    summary_prompt: String,
    response_attempt_ordinal: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl SessionCompaction {
    pub(crate) fn begin_turn(
        authority: SessionAuthorityHandle,
        turn_id: Uuid,
        step_id: Uuid,
        trigger: CompactionTrigger,
        evict_count: usize,
    ) -> Result<Option<Self>, AuthorityError> {
        Self::begin(
            authority,
            CompactionOwnerScope::Turn { turn_id, step_id },
            trigger,
            evict_count,
        )
    }

    pub(crate) fn begin_idle(
        authority: SessionAuthorityHandle,
        evict_count: usize,
    ) -> Result<Option<Self>, AuthorityError> {
        Self::begin(
            authority,
            CompactionOwnerScope::SessionIdle,
            CompactionTrigger::ManualIdle,
            evict_count,
        )
    }

    fn begin(
        authority: SessionAuthorityHandle,
        owner_scope: CompactionOwnerScope,
        trigger: CompactionTrigger,
        evict_count: usize,
    ) -> Result<Option<Self>, AuthorityError> {
        let state = authority.state();
        let Some((request_id, request)) = state
            .model_requests
            .iter()
            .filter_map(|(request_id, request)| {
                let event_id = state.model_request_source_events.get(request_id)?;
                let sequence = state
                    .command_receipts
                    .values()
                    .find(|receipt| receipt.event_id == *event_id)?
                    .sequence;
                Some((sequence, *request_id, request.preparation()))
            })
            .max_by_key(|(sequence, _, _)| *sequence)
            .map(|(_, request_id, request)| (request_id, request))
        else {
            return Ok(None);
        };
        let source_event_id = state.model_request_source_events[&request_id];
        let conversation_items = request
            .context_items
            .iter()
            .filter(|item| item.role != ModelContextRole::System)
            .collect::<Vec<_>>();
        if evict_count == 0 || conversation_items.len() <= evict_count {
            return Ok(None);
        }
        let (prompt_owner, prompt_generation, summary_prompt) =
            summary_prompt().map_err(|error| AuthorityError::Invalid(error.to_string()))?;
        let make_item = |ordinal: usize,
                         item: &crate::session_authority::ModelContextItem|
         -> Result<CompactionContextItem, AuthorityError> {
            Ok(CompactionContextItem {
                ordinal: u32::try_from(ordinal).map_err(|_| {
                    AuthorityError::Invalid("compaction item ordinal exceeds u32".into())
                })?,
                source_event_id,
                source_identity: format!("{request_id}:{}", item.ordinal),
                content_ref: item.content_ref.clone(),
            })
        };
        let input_items = conversation_items[..evict_count]
            .iter()
            .enumerate()
            .map(|(ordinal, item)| make_item(ordinal, item))
            .collect::<Result<Vec<_>, _>>()?;
        let retained_items = conversation_items[evict_count..]
            .iter()
            .enumerate()
            .map(|(ordinal, item)| make_item(ordinal, item))
            .collect::<Result<Vec<_>, _>>()?;
        let mut provider_payload = String::new();
        for item in &input_items {
            let bytes = authority.read_content(&item.content_ref, ProjectionClass::Default)?;
            let text = std::str::from_utf8(&bytes).map_err(|_| {
                AuthorityError::Invalid("compaction input is not valid UTF-8".into())
            })?;
            if !provider_payload.is_empty() {
                provider_payload.push('\n');
            }
            provider_payload.push_str(text);
        }

        let compaction_id = Uuid::new_v4();
        let compaction_request_id = Uuid::new_v4();
        let source_context_revision = state.context_revision;
        let target_context_revision = source_context_revision.checked_add(1).ok_or_else(|| {
            AuthorityError::Invalid("compaction context revision overflow".into())
        })?;
        let mut start = CompactionStarted {
            compaction_id,
            owner_scope: owner_scope.clone(),
            trigger,
            source_frontier: AuthorityFrontierRef {
                sequence: state.last_sequence,
                event_id: state.last_event_id.ok_or_else(|| {
                    AuthorityError::Invalid("compaction source frontier is empty".into())
                })?,
            },
            source_context_revision,
            input_manifest_id: String::new(),
            input_items,
            retained_items: retained_items.clone(),
            target_context_revision,
        };
        start.input_manifest_id = compaction_input_manifest_id(&start)?;
        authority.start_compaction(Uuid::new_v4(), &recorded_at_now(), start)?;
        Ok(Some(Self {
            authority,
            compaction_id,
            compaction_request_id,
            owner_scope,
            source_context_revision,
            target_context_revision,
            retained_items,
            provider_payload,
            prompt_owner,
            prompt_generation,
            summary_prompt,
            response_attempt_ordinal: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }))
    }

    pub(crate) fn compaction_request_id(&self) -> Uuid {
        self.compaction_request_id
    }

    pub(crate) fn owner_scope(&self) -> &CompactionOwnerScope {
        &self.owner_scope
    }

    pub(crate) fn provider_payload(&self) -> &str {
        &self.provider_payload
    }

    pub(crate) fn prepare(&self, route: CompactionRoute) -> Result<(), AuthorityError> {
        let prompt_ref = self.authority.write_content(
            self.summary_prompt.as_bytes(),
            "text/plain",
            ProjectionClass::Default,
        )?;
        self.authority.prepare_compaction_request(
            Uuid::new_v4(),
            &recorded_at_now(),
            CompactionRequestPrepared {
                compaction_request_id: self.compaction_request_id,
                compaction_id: self.compaction_id,
                request_ordinal: 0,
                replaces_compaction_request_id: None,
                prompt_template: CompactionPromptTemplate {
                    owner_id: self.prompt_owner.clone(),
                    owner_generation_id: self.prompt_generation.clone(),
                    content_ref: prompt_ref,
                },
                route,
            },
        )?;
        Ok(())
    }

    pub(crate) fn fail_response_attempt(
        &self,
        failure: CompactionResponseAttemptFailure,
        reason_code: &str,
    ) -> Result<(), AuthorityError> {
        let attempt = self
            .response_attempt_ordinal
            .load(std::sync::atomic::Ordering::Acquire);
        let next = attempt.checked_add(1).ok_or_else(|| {
            AuthorityError::Invalid("compaction response-attempt ordinal overflow".into())
        })?;
        self.authority.fail_compaction_response_attempt(
            Uuid::new_v4(),
            &recorded_at_now(),
            CompactionResponseAttemptFailed {
                compaction_request_id: self.compaction_request_id,
                compaction_id: self.compaction_id,
                response_attempt_ordinal: attempt,
                failure,
                reason_code: reason_code.into(),
                retry_disposition: CompactionRetryDisposition::RetrySameRequest,
            },
        )?;
        self.response_attempt_ordinal
            .store(next, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    pub(crate) fn commit_done(
        &self,
        summary: &str,
        usage: Option<CompactionUsage>,
    ) -> Result<(), AuthorityError> {
        if summary.is_empty() {
            return Err(AuthorityError::Invalid(
                "provider Done compaction summary is empty".into(),
            ));
        }
        let summary_ref = self.authority.write_content(
            summary.as_bytes(),
            "text/plain",
            ProjectionClass::Default,
        )?;
        let summary_id = Uuid::new_v4();
        let mut replacement_items = Vec::with_capacity(self.retained_items.len() + 1);
        replacement_items.push(CompactionReplacementItem {
            ordinal: 0,
            source_kind: CompactionReplacementSourceKind::CompactionSummary,
            source_event_id: Uuid::nil(),
            source_identity: summary_id.to_string(),
            content_ref: summary_ref.clone(),
        });
        replacement_items.extend(self.retained_items.iter().enumerate().map(
            |(index, retained)| CompactionReplacementItem {
                ordinal: u32::try_from(index + 1).expect("retained ordinal was validated"),
                source_kind: CompactionReplacementSourceKind::Retained,
                source_event_id: retained.source_event_id,
                source_identity: retained.source_identity.clone(),
                content_ref: retained.content_ref.clone(),
            },
        ));
        self.authority.commit_compaction_summary(
            Uuid::new_v4(),
            &recorded_at_now(),
            CompactionSummaryCommitted {
                compaction_summary_id: summary_id,
                compaction_request_id: self.compaction_request_id,
                compaction_id: self.compaction_id,
                response_attempt_ordinal: self
                    .response_attempt_ordinal
                    .load(std::sync::atomic::Ordering::Acquire),
                completion_evidence: ProviderCompletionEvidence::ProviderDone,
                summary_digest: format!("{:x}", Sha256::digest(summary.as_bytes())),
                summary_ref,
                replacement_manifest_id: String::new(),
                replacement_items,
                usage,
            },
        )?;
        self.authority.close_compaction_request(
            Uuid::new_v4(),
            &recorded_at_now(),
            CompactionRequestClosed {
                compaction_request_id: self.compaction_request_id,
                compaction_id: self.compaction_id,
                response_attempt_ordinal: self
                    .response_attempt_ordinal
                    .load(std::sync::atomic::Ordering::Acquire),
                outcome: CompactionRequestOutcome::SummaryCommitted,
                reason_code: "provider_done".into(),
                recovery_rule_version: None,
            },
        )?;
        let state = self.authority.state();
        let committed = &state.compaction_summaries[&summary_id];
        self.authority.apply_compaction(
            Uuid::new_v4(),
            &recorded_at_now(),
            CompactionApplied {
                compaction_id: self.compaction_id,
                compaction_summary_id: summary_id,
                source_context_revision: self.source_context_revision,
                target_context_revision: self.target_context_revision,
                replacement_manifest_id: committed.replacement_manifest_id.clone(),
                recovery_rule_version: None,
            },
        )?;
        Ok(())
    }

    pub(crate) fn fail(
        &self,
        outcome: CompactionRequestOutcome,
        reason_code: &str,
    ) -> Result<(), AuthorityError> {
        if outcome == CompactionRequestOutcome::SummaryCommitted
            || outcome == CompactionRequestOutcome::SupersededForRouteChange
        {
            return Err(AuthorityError::Invalid(
                "terminal compaction failure outcome is invalid".into(),
            ));
        }
        self.authority.close_compaction_request(
            Uuid::new_v4(),
            &recorded_at_now(),
            CompactionRequestClosed {
                compaction_request_id: self.compaction_request_id,
                compaction_id: self.compaction_id,
                response_attempt_ordinal: self
                    .response_attempt_ordinal
                    .load(std::sync::atomic::Ordering::Acquire),
                outcome,
                reason_code: reason_code.into(),
                recovery_rule_version: None,
            },
        )?;
        self.authority.abandon_compaction(
            Uuid::new_v4(),
            &recorded_at_now(),
            CompactionAbandoned {
                compaction_id: self.compaction_id,
                reason_code: reason_code.into(),
                last_compaction_request_id: Some(self.compaction_request_id),
                last_response_attempt_ordinal: Some(
                    self.response_attempt_ordinal
                        .load(std::sync::atomic::Ordering::Acquire),
                ),
                recovery_rule_version: 1,
            },
        )?;
        Ok(())
    }
}

fn recorded_at_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl crate::loop_driver::LoopCompactionAuthority for SessionCompaction {
    fn provider_payload<'a>(&'a self, _fallback: &'a str) -> &'a str {
        self.provider_payload()
    }

    fn compaction_request_id(&self) -> Option<Uuid> {
        Some(self.compaction_request_id())
    }

    fn is_idle(&self) -> bool {
        matches!(self.owner_scope(), CompactionOwnerScope::SessionIdle)
    }

    fn prepare(
        &self,
        evidence: crate::loop_driver::LoopCompactionRouteEvidence,
    ) -> anyhow::Result<()> {
        let route = match self.owner_scope() {
            CompactionOwnerScope::Turn { .. } => CompactionRoute::TurnLease {
                lease_id: evidence
                    .lease_id
                    .ok_or_else(|| anyhow::anyhow!("turn compaction route lease is absent"))?,
            },
            CompactionOwnerScope::SessionIdle => {
                if evidence.lease_id.is_some() {
                    anyhow::bail!("idle compaction cannot claim a turn route lease");
                }
                CompactionRoute::SessionIdle {
                    selected_provider_id: evidence.selected_provider_id,
                    selected_model_id: evidence.selected_model_id,
                    serving_provider_id: evidence.serving_provider_id,
                    serving_model_id: evidence.serving_model_id,
                    schema_dialect: evidence.schema_dialect,
                    credential_source_class: evidence.credential_source_class,
                    fallback_reason: evidence.fallback_reason,
                    contribution_generation_id: evidence.contribution_generation_id,
                    route_policy: evidence.route_policy,
                }
            }
        };
        self.prepare(route).map_err(anyhow::Error::from)
    }

    fn commit_done(&self, summary: &str) -> anyhow::Result<()> {
        self.commit_done(summary, None).map_err(anyhow::Error::from)
    }

    fn fail(&self, outcome: CompactionRequestOutcome, reason: &str) -> anyhow::Result<()> {
        self.fail(outcome, reason).map_err(anyhow::Error::from)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn summary_prompt_carries_admitted_content_generation() {
        let (owner, generation, body) = super::summary_prompt().unwrap();
        assert_eq!(owner, "content-pack:omegon-shipped");
        assert!(generation.starts_with("content:omegon-shipped@1.0.0:"));
        assert!(body.contains("conversation summarizer"));
    }

    #[test]
    fn absent_pack_disables_compaction_prompt_locally() {
        let error = super::summary_prompt_from_pack(None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "shipped compaction prompt is unavailable"
        );
    }
}
