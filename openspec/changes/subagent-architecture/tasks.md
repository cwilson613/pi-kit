# Subagent architecture — Tasks

## 1. Delegate execution engine (core/crates/omegon/src/delegate/)

- [ ] 1.1 agent_loader.rs: parse .omegon/agents/*.md — frontmatter (name, description, model, tools, scope, mind) + body as system prompt. Scan at startup, return Vec<AgentSpec>.
- [ ] 1.2 field_kit.rs: assemble child context — merge agent defaults with per-invocation overrides (model, thinking_level, scope, facts, mind). Pull specific facts from MemoryBackend by ID or query. Build task file content.
- [ ] 1.3 runner.rs: spawn delegate child — check agent tool list for write tools → worktree or in-place. Reuse cleave worktree infra for write agents. For read-only: spawn headless child in cwd. Capture output. Enforce max 4 concurrent async delegates.
- [ ] 1.4 result_store.rs: store/retrieve async results by task_id — HashMap<String, DelegateResult> with status (running/completed/failed), output text, elapsed time. Thread-safe (Arc<Mutex>).

## 2. DelegateFeature (core/crates/omegon/src/features/delegate.rs)

- [ ] 2.1 delegate tool: accept task, agent (optional name), scope, model, thinking_level, facts, mind, background. Sync (background=false) → block + return result as ToolResult. Async (background=true) → spawn runner, return task_id immediately.
- [ ] 2.2 delegate_result tool: accept task_id, return stored output or "still running" status.
- [ ] 2.3 delegate_status tool: list all active + recently completed delegates with name, status, elapsed.
- [ ] 2.4 on_event: emit BusRequest::Notify on async delegate completion (toast in TUI).
- [ ] 2.5 provide_context: inject available agent names + descriptions so the LLM knows what specialists exist.

## 3. Wiring (mod.rs + setup.rs + tui/mod.rs)

- [ ] 3.1 features/mod.rs: register delegate module
- [ ] 3.2 setup.rs: register DelegateFeature with Arc<dyn MemoryBackend> + cleave worktree handles
- [ ] 3.3 tui/mod.rs: add /delegate slash command (status view), handle delegate completion toast, add "delegate" to COMMANDS table

## 4. Tests

- [ ] 4.1 agent_loader: parse valid .md, missing frontmatter, no agents dir
- [ ] 4.2 field_kit: merge defaults + overrides, fact extraction
- [ ] 4.3 runner: read-only detection (no worktree), write detection (worktree), concurrent limit
- [ ] 4.4 result_store: store/retrieve/status lifecycle
- [ ] 4.5 delegate tool: sync execution, async task_id return, unknown agent, missing task
- [ ] 4.6 delegate_result: retrieve completed, retrieve running, retrieve nonexistent
