//! Compatibility implementation for the release-coupled loop context boundary.

use crate::bridge::LlmMessage;
use crate::context::{ContextManager, PromptTelemetry};
use crate::conversation::{ConversationState, ToolCall};
use crate::r#loop::LoopConfig;
use crate::util::estimate_chars_to_tokens;

const AUTO_PRESSURE_COMPACTION_KEEP_RECENT_TURNS: u32 = 4;

#[derive(Debug, Clone)]
pub(crate) struct LoopContextWindows {
    pub(crate) provider_window: usize,
    pub(crate) assembly_window: usize,
}

pub(crate) struct LoopContextAssembly {
    pub(crate) system_prompt: String,
    pub(crate) messages: Vec<LlmMessage>,
    pub(crate) composition: omegon_traits::ContextComposition,
}

pub(crate) struct LoopContextUpdate {
    pub(crate) tokens: u64,
    pub(crate) context_window: u64,
    pub(crate) context_class: String,
    pub(crate) thinking_level: String,
}

#[derive(Debug, Clone, Copy)]
enum CompactionApplication {
    DecayWindow,
    KeepRecent(u32),
}

pub(crate) struct LoopCompactionPlan {
    pub(crate) payload: String,
    pub(crate) evict_count: usize,
    pub(crate) reason: Option<String>,
    application: CompactionApplication,
}

pub(crate) struct LoopContextCompatibilityAdapter<'a> {
    manager: &'a mut ContextManager,
}

impl<'a> LoopContextCompatibilityAdapter<'a> {
    pub(crate) fn new(manager: &'a mut ContextManager) -> Self {
        Self { manager }
    }

    pub(crate) fn resolve_windows(&mut self, config: &LoopConfig) -> LoopContextWindows {
        if let Some(settings) = config
            .settings
            .as_ref()
            .and_then(|settings| settings.lock().ok().map(|guard| guard.clone()))
        {
            let policy = settings.selector_policy();
            let windows = LoopContextWindows {
                provider_window: settings.context_window,
                assembly_window: policy.assembly_window(),
            };
            self.manager.set_selector_policy(policy);
            windows
        } else {
            self.manager.set_context_window(200_000);
            LoopContextWindows {
                provider_window: 200_000,
                assembly_window: 200_000,
            }
        }
    }

    pub(crate) async fn prepare_turn(
        &mut self,
        conversation: &mut ConversationState,
        runtime: &mut crate::bus::EventBus,
        turn: u32,
        tools: &[omegon_traits::ToolDefinition],
        context_window: usize,
    ) -> LoopContextAssembly {
        if conversation.intent.stats.tool_calls > 0
            || conversation.intent.current_task.is_some()
            || conversation.intent.stats.compactions > 0
            || conversation.intent.has_active_work_plan_context()
        {
            self.manager
                .inject_intent(conversation.render_intent_for_injection());
        }

        let user_prompt = conversation.last_user_prompt();
        runtime.emit(&omegon_traits::BusEvent::ContextBuild {
            user_prompt: user_prompt.to_string(),
            turn,
        });
        let (recent_tools, recent_files, budget) = self.manager.signals_data();
        let signals = omegon_traits::ContextSignals {
            user_prompt,
            recent_tools: &recent_tools,
            recent_files: &recent_files,
            lifecycle_phase: self.manager.phase(),
            turn_number: turn,
            context_budget_tokens: budget,
        };
        let injections = runtime.collect_context(&signals);
        if !injections.is_empty() {
            tracing::debug!(count = injections.len(), "bus context injections");
            self.manager.inject_external(injections);
        }

        if let Some(content) = conversation.render_attachment_context_injection() {
            self.manager
                .inject_external(vec![omegon_traits::ContextInjection {
                    source: "attachment-files".into(),
                    content,
                    priority: 190,
                    ttl_turns: 1,
                }]);
        }

        self.manager.prepare_embeddings(user_prompt).await;
        self.compose(conversation, tools, context_window)
    }

    pub(crate) fn compose(
        &mut self,
        conversation: &ConversationState,
        tools: &[omegon_traits::ToolDefinition],
        context_window: usize,
    ) -> LoopContextAssembly {
        compose_with_manager(self.manager, conversation, tools, context_window)
    }

    pub(crate) fn messages(&self, conversation: &ConversationState) -> Vec<LlmMessage> {
        conversation.build_llm_view()
    }

    pub(crate) fn record_activity(&mut self, calls: &[ToolCall]) {
        for call in calls {
            self.manager.record_tool_call(&call.name);
            if let Some(path) = call
                .arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
            {
                self.manager
                    .record_file_access(std::path::PathBuf::from(path));
            }
        }
        self.manager.update_phase_from_activity(calls);
    }

    pub(crate) fn pressure_compaction_plan(
        &self,
        conversation: &ConversationState,
    ) -> Option<LoopCompactionPlan> {
        if let Some((payload, evict_count)) = conversation.build_compaction_payload() {
            return Some(LoopCompactionPlan {
                payload,
                evict_count,
                reason: None,
                application: CompactionApplication::DecayWindow,
            });
        }
        conversation
            .build_compaction_payload_keeping_recent(AUTO_PRESSURE_COMPACTION_KEEP_RECENT_TURNS)
            .map(|(payload, evict_count)| LoopCompactionPlan {
                payload,
                evict_count,
                reason: Some(format!(
                    "no decay-window payload; compacting under token pressure with keep_recent_turns={AUTO_PRESSURE_COMPACTION_KEEP_RECENT_TURNS}"
                )),
                application: CompactionApplication::KeepRecent(
                    AUTO_PRESSURE_COMPACTION_KEEP_RECENT_TURNS,
                ),
            })
    }

    pub(crate) fn overflow_compaction_plan(
        &self,
        conversation: &ConversationState,
    ) -> Option<LoopCompactionPlan> {
        conversation
            .build_compaction_payload()
            .map(|(payload, evict_count)| LoopCompactionPlan {
                payload,
                evict_count,
                reason: None,
                application: CompactionApplication::DecayWindow,
            })
    }

    pub(crate) fn apply_compaction(
        &self,
        conversation: &mut ConversationState,
        plan: LoopCompactionPlan,
        summary: String,
    ) {
        match plan.application {
            CompactionApplication::DecayWindow => conversation.apply_compaction(summary),
            CompactionApplication::KeepRecent(turns) => {
                conversation.apply_compaction_keeping_recent(summary, turns);
            }
        }
    }

    pub(crate) fn decay_failed_compaction(
        &self,
        conversation: &mut ConversationState,
        plan: &LoopCompactionPlan,
    ) {
        conversation.decay_oldest(plan.evict_count);
    }

    pub(crate) fn repair_overflow_without_plan(
        &self,
        conversation: &mut ConversationState,
    ) -> usize {
        let evict_count = conversation.message_count() / 2;
        conversation.decay_oldest(evict_count);
        evict_count
    }

    pub(crate) fn repair_malformed_history(&self, conversation: &mut ConversationState) {
        let half = conversation.message_count() / 2;
        conversation.decay_oldest(half.max(1));
    }

    pub(crate) fn tighten_decay(&self, conversation: &mut ConversationState) {
        conversation.tighten_decay();
    }

    pub(crate) fn context_update(
        &self,
        config: &LoopConfig,
        conversation: &ConversationState,
        context_window: usize,
    ) -> LoopContextUpdate {
        LoopContextUpdate {
            tokens: conversation.estimate_tokens() as u64,
            context_window: context_window as u64,
            context_class: config
                .settings
                .as_ref()
                .and_then(|settings| {
                    settings
                        .lock()
                        .ok()
                        .map(|guard| guard.context_class.label().to_string())
                })
                .unwrap_or_else(|| {
                    crate::settings::ContextClass::from_tokens(context_window)
                        .label()
                        .to_string()
                }),
            thinking_level: config
                .settings
                .as_ref()
                .and_then(|settings| {
                    settings
                        .lock()
                        .ok()
                        .map(|guard| guard.thinking.as_str().to_string())
                })
                .unwrap_or_else(|| "off".to_string()),
        }
    }
}

pub(crate) fn compose_with_manager(
    manager: &mut ContextManager,
    conversation: &ConversationState,
    tools: &[omegon_traits::ToolDefinition],
    context_window: usize,
) -> LoopContextAssembly {
    let system_prompt = manager.build_system_prompt(conversation.last_user_prompt(), conversation);
    let messages = conversation.build_llm_view();
    let telemetry = manager.last_prompt_telemetry();
    let composition =
        compute_context_composition(&system_prompt, &messages, tools, context_window, &telemetry);
    LoopContextAssembly {
        system_prompt,
        messages,
        composition,
    }
}

pub(crate) fn default_context_composition(
    context_window: usize,
) -> omegon_traits::ContextComposition {
    omegon_traits::ContextComposition {
        free_tokens: context_window,
        ..omegon_traits::ContextComposition::default()
    }
}

fn estimate_tool_schema_tokens(tools: &[omegon_traits::ToolDefinition]) -> usize {
    tools
        .iter()
        .map(|tool| {
            let schema_json = serde_json::to_string(&tool.parameters).unwrap_or_default();
            estimate_chars_to_tokens(tool.name.len() + tool.description.len() + schema_json.len())
        })
        .sum()
}

fn compute_context_composition(
    system_prompt: &str,
    messages: &[LlmMessage],
    tools: &[omegon_traits::ToolDefinition],
    context_window: usize,
    telemetry: &PromptTelemetry,
) -> omegon_traits::ContextComposition {
    let system_tokens = estimate_chars_to_tokens(system_prompt.len());
    let tool_schema_tokens = estimate_tool_schema_tokens(tools);
    let mut conversation_tokens = 0usize;
    let mut memory_tokens = 0usize;
    let mut tool_history_tokens = 0usize;
    let mut thinking_tokens = 0usize;

    for message in messages {
        match message {
            LlmMessage::User { content, .. } => {
                conversation_tokens += estimate_chars_to_tokens(content.len());
            }
            LlmMessage::Assistant {
                text,
                thinking,
                tool_calls,
                ..
            } => {
                conversation_tokens +=
                    estimate_chars_to_tokens(text.iter().map(String::len).sum::<usize>());
                thinking_tokens +=
                    estimate_chars_to_tokens(thinking.iter().map(String::len).sum::<usize>());
                tool_history_tokens += estimate_chars_to_tokens(
                    tool_calls
                        .iter()
                        .map(|call| call.name.len() + call.arguments.to_string().len())
                        .sum::<usize>(),
                );
            }
            LlmMessage::ToolResult {
                content,
                tool_name,
                images,
                ..
            } => {
                let image_chars = images
                    .iter()
                    .map(|image| image.data.len() + image.media_type.len())
                    .sum::<usize>();
                tool_history_tokens +=
                    estimate_chars_to_tokens(content.len() + tool_name.len() + image_chars);
                if is_declared_memory_tool(tool_name) {
                    memory_tokens += estimate_chars_to_tokens(content.len());
                }
            }
        }
    }

    let used = system_tokens
        .saturating_add(conversation_tokens)
        .saturating_add(memory_tokens)
        .saturating_add(tool_schema_tokens)
        .saturating_add(tool_history_tokens)
        .saturating_add(thinking_tokens);
    omegon_traits::ContextComposition {
        conversation_tokens,
        system_tokens,
        memory_tokens,
        tool_schema_tokens,
        tool_history_tokens,
        thinking_tokens,
        free_tokens: context_window.saturating_sub(used),
        base_prompt_tokens: estimate_chars_to_tokens(telemetry.base_prompt_chars),
        session_hud_tokens: estimate_chars_to_tokens(telemetry.session_hud_chars),
        intent_tokens: estimate_chars_to_tokens(telemetry.intent_chars),
        external_injection_tokens: estimate_chars_to_tokens(telemetry.external_injection_chars),
        tool_guidance_tokens: estimate_chars_to_tokens(telemetry.tool_guidance_chars),
        file_guidance_tokens: estimate_chars_to_tokens(telemetry.file_guidance_chars),
    }
}

fn is_declared_memory_tool(name: &str) -> bool {
    use crate::tool_registry::memory;
    matches!(
        name,
        memory::MEMORY_STORE
            | memory::MEMORY_RECALL
            | memory::MEMORY_QUERY
            | memory::MEMORY_ARCHIVE
            | memory::MEMORY_SUPERSEDE
            | memory::MEMORY_CONNECT
            | memory::MEMORY_FOCUS
            | memory::MEMORY_RELEASE
            | memory::MEMORY_EPISODES
            | memory::MEMORY_COMPACT
            | memory::MEMORY_SEARCH_ARCHIVE
            | memory::MEMORY_INGEST_LIFECYCLE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_attribution_uses_declared_tools_not_name_prefixes() {
        let messages = vec![
            LlmMessage::ToolResult {
                call_id: "declared".into(),
                tool_name: crate::tool_registry::memory::MEMORY_RECALL.into(),
                content: "durable context".into(),
                images: vec![],
                is_error: false,
                args_summary: None,
            },
            LlmMessage::ToolResult {
                call_id: "lookalike".into(),
                tool_name: "memory_untrusted_plugin".into(),
                content: "not runtime memory".into(),
                images: vec![],
                is_error: false,
                args_summary: None,
            },
        ];

        let composition = compute_context_composition(
            "system",
            &messages,
            &[],
            1_000,
            &PromptTelemetry::default(),
        );

        assert_eq!(
            composition.memory_tokens,
            estimate_chars_to_tokens("durable context".len())
        );
    }

    #[test]
    fn pressure_compaction_falls_back_before_decay_window() {
        let mut manager = ContextManager::new(String::new(), vec![]);
        let adapter = LoopContextCompatibilityAdapter::new(&mut manager);
        let mut conversation = ConversationState::new();
        conversation.push_user("turn zero context".into());
        conversation.intent.stats.turns = 1;
        conversation.push_user("turn one context".into());
        conversation.intent.stats.turns = 6;
        conversation.push_user("recent context".into());

        let plan = adapter
            .pressure_compaction_plan(&conversation)
            .expect("pressure plan");

        assert_eq!(plan.evict_count, 2);
        assert!(plan.payload.contains("turn zero context"));
        assert!(plan.payload.contains("turn one context"));
        assert!(!plan.payload.contains("recent context"));
        assert!(plan.reason.is_some());
    }

    #[test]
    fn pressure_compaction_prefers_decay_window_and_applies_summary() {
        let mut manager = ContextManager::new(String::new(), vec![]);
        let adapter = LoopContextCompatibilityAdapter::new(&mut manager);
        let mut conversation = ConversationState::new();
        conversation.push_user("very old context".into());
        conversation.intent.stats.turns = 99;
        conversation.push_user("recent context".into());

        let plan = adapter
            .pressure_compaction_plan(&conversation)
            .expect("pressure plan");
        assert_eq!(plan.evict_count, 1);
        assert!(plan.reason.is_none());

        adapter.apply_compaction(&mut conversation, plan, "retained summary".into());

        let replay = conversation.build_llm_view();
        assert!(replay.iter().any(|message| {
            matches!(message, LlmMessage::User { content, .. } if content.contains("retained summary"))
        }));
        assert_eq!(conversation.intent.stats.compactions, 1);
    }

    #[test]
    fn malformed_history_repair_is_bounded_and_keeps_a_legal_view() {
        let mut manager = ContextManager::new(String::new(), vec![]);
        let adapter = LoopContextCompatibilityAdapter::new(&mut manager);
        let mut conversation = ConversationState::new();
        conversation.push_user("oldest".into());
        conversation.push_user("middle".into());
        conversation.push_user("newest".into());
        let before = conversation.message_count();

        adapter.repair_malformed_history(&mut conversation);

        assert!(conversation.message_count() < before);
        assert!(!conversation.build_llm_view().is_empty());
    }

    #[test]
    fn overflow_repair_without_compaction_plan_decays_exactly_half_the_history() {
        let mut manager = ContextManager::new(String::new(), vec![]);
        let adapter = LoopContextCompatibilityAdapter::new(&mut manager);
        let mut conversation = ConversationState::new();
        for message in ["oldest", "older", "recent", "newest"] {
            conversation.push_user(message.into());
        }
        let before = conversation.message_count();

        let evicted = adapter.repair_overflow_without_plan(&mut conversation);

        assert_eq!(evicted, before / 2);
        assert_eq!(conversation.message_count(), before - evicted);
        assert_eq!(conversation.last_user_prompt(), "newest");
    }

    #[test]
    fn selector_resolution_uses_requested_assembly_window() {
        let settings = crate::settings::shared("anthropic:claude-sonnet-4-6");
        settings
            .lock()
            .unwrap()
            .set_requested_context_class(crate::settings::ContextClass::Compact);
        let config = LoopConfig {
            settings: Some(settings),
            ..LoopConfig::default()
        };
        let mut manager = ContextManager::new(String::new(), vec![]);
        let mut adapter = LoopContextCompatibilityAdapter::new(&mut manager);

        let windows = adapter.resolve_windows(&config);

        assert!(windows.provider_window > windows.assembly_window);
        assert_eq!(
            windows.assembly_window,
            crate::settings::ContextClass::Compact.nominal_tokens()
        );
    }
}
