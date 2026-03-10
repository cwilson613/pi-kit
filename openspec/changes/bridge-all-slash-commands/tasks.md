# bridge-all-slash-commands — Tasks

## 1. Bridge OpenSpec commands
<!-- specs: harness/slash-command-bridge -->
<!-- skills: typescript -->

- [ ] 1.1 Create a shared SlashCommandBridge instance in extensions/openspec/index.ts (or import from a shared location)
- [ ] 1.2 Convert /opsx:status to bridged command with structuredExecutor returning changes array
- [ ] 1.3 Convert /opsx:verify to bridged command with structuredExecutor returning verification substate
- [ ] 1.4 Convert /opsx:archive to bridged command with structuredExecutor (sideEffectClass: workspace-write)
- [ ] 1.5 Convert /opsx:propose to bridged command with structuredExecutor (sideEffectClass: workspace-write)
- [ ] 1.6 Convert /opsx:spec to bridged command with structuredExecutor (sideEffectClass: workspace-write)
- [ ] 1.7 Convert /opsx:ff to bridged command with structuredExecutor (sideEffectClass: workspace-write)
- [ ] 1.8 Convert /opsx:apply to bridged command with structuredExecutor (sideEffectClass: read)
- [ ] 1.9 Write regression tests for bridged OpenSpec commands in extensions/openspec/bridge.test.ts

## 2. Register interactive-only commands with agentCallable: false
<!-- skills: typescript -->

- [ ] 2.1 Register /dashboard with bridge (agentCallable: false) so it returns structured refusal instead of opaque "not registered"
- [ ] 2.2 Audit remaining commands and register any interactive-only ones with agentCallable: false
- [ ] 2.3 Write test for structured refusal of interactive-only commands

## 3. Verify side-effect metadata and interactive UX preservation
<!-- specs: harness/slash-command-bridge -->
<!-- skills: typescript -->

- [ ] 3.1 Verify read-only commands (opsx:status, opsx:verify) declare sideEffectClass: read
- [ ] 3.2 Verify write commands (opsx:propose, opsx:ff, opsx:archive) declare sideEffectClass: workspace-write
- [ ] 3.3 Verify interactive /opsx:status and /opsx:verify render from structuredExecutor result
- [ ] 3.4 Run full test suite (npm run check) to confirm no regressions
