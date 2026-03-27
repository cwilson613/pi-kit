---
id: sovereign-multi-repo-project-management
title: Sovereign multi-repo project management on top of omegon-design
status: exploring
parent: git-native-task-management
tags: [forgejo, multi-repo, sovereign, project-management, web]
open_questions: []
jj_change_id: urroornuzoyklopmyzxtuytzwknnxtqp
issue_type: epic
priority: 3
---

# Sovereign multi-repo project management on top of omegon-design

## Overview

Use the extracted omegon-design crate as the domain layer for a separate project-management application that aggregates multiple repos, likely alongside Forgejo. Provide a unified cross-project view while keeping each repo's `.omegon/design/` as the source of truth.

## Research

### Reference project: claude-devtools

claude-devtools is an adjacent reference point, but it solves a different layer of the stack. Based on its site and GitHub README summaries, it is a read-only inspector for Claude Code sessions rather than a project-management system.

What it appears to do:
- Reads raw Claude Code session logs from `~/.claude/`.
- Reconstructs execution traces: file reads, regex searches, edit diffs, bash output, subagent trees, and token usage.
- Rebuilds per-turn context composition from recorded artifacts such as `CLAUDE.md` injections, skill activations, `@`-mentioned files, tool I/O, thinking, team overhead, and user text.
- Presents that reconstruction in a searchable desktop/web UI, including SSH access to inspect remote machines.

How it gains its 'insights':
- Not by instrumenting or wrapping Claude Code in real time.
- Not by proprietary provider-side introspection.
- By parsing durable local session artifacts Claude Code already writes, then replaying / classifying them into higher-level categories.

Implication for Omegon:
- This validates the product value of post-hoc observability over agent sessions.
- But its source of truth is session telemetry, whereas sovereign multi-repo project management should use repo-native state (`.omegon/design/`, OpenSpec changes, milestones, sessions) as the primary model.
- The strongest analogue for us is an inspector/dashboard layer over our own durable artifacts, not a wrapper around live execution.

## Open Questions

*No open questions.*
