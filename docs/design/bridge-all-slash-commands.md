---
id: bridge-all-slash-commands
title: Bridge all pi-kit slash commands through SlashCommandBridge
status: implemented
parent: agent-assess-tooling-access
tags: [harness, slash-commands, bridge, openspec, agent-callable]
open_questions: []
---

# Bridge all pi-kit slash commands through SlashCommandBridge

## Overview

Convert all pi-kit slash commands to use the SlashCommandBridge so the agent can invoke them via execute_slash_command. Currently only /assess is bridged, causing repeated failures when the agent tries lifecycle commands like /opsx:verify and /opsx:archive.

## Decisions

### Decision: Share a single SlashCommandBridge instance across all extensions

**Status:** decided
**Rationale:** Creating separate bridges per extension would split the execute_slash_command tool's command list and make some commands invisible to the agent. A shared singleton ensures all bridged commands are discoverable through one tool.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `extensions/openspec/index.ts` (modified) — Post-assess reconciliation delta — touched during follow-up fixes
- `extensions/openspec/bridge.test.ts` (modified) — Post-assess reconciliation delta — touched during follow-up fixes
- `extensions/lib/slash-command-bridge.ts` (modified) — Post-assess reconciliation delta — touched during follow-up fixes
- `extensions/cleave/index.ts` (modified) — Post-assess reconciliation delta — touched during follow-up fixes
- `extensions/dashboard/index.ts` (modified) — Post-assess reconciliation delta — touched during follow-up fixes

### Constraints

- All pi-kit slash commands must be registered with the shared SlashCommandBridge (getSharedBridge()) so execute_slash_command can discover and invoke them.
- Interactive-only commands are bridged with agentCallable: false for structured refusals.
