---
id: rc1-repo-backed-orchestration-proof
title: "RC1: repo-backed orchestration proof"
status: exploring
parent: release-0-15-4-trust-hardening
tags: [release, rc1, cleave, verification]
open_questions:
  - "Which real repo-backed task should serve as the rc.1 proof case so it exercises routing, child execution, and final reporting without depending on an artificial scratch scenario?"
  - "What must be true at the end of the proof run for rc.1 acceptance — successful child completion, accurate provider/model reporting, expected file changes or no-op rationale, and no merge/worktree bookkeeping contradiction?"
dependencies: []
related:
  - orchestratable-provider-model
---

# RC1: repo-backed orchestration proof

## Overview

Release-checklist node for the third rc.1 acceptance criterion: at least one realistic repo-backed orchestrated execution path must succeed end-to-end and leave state that matches what the operator sees. This node exists to avoid repeating the false confidence of synthetic scratch probes that do not exercise the full routing, child execution, and reporting path.

## Open Questions

- Which real repo-backed task should serve as the rc.1 proof case so it exercises routing, child execution, and final reporting without depending on an artificial scratch scenario?
- What must be true at the end of the proof run for rc.1 acceptance — successful child completion, accurate provider/model reporting, expected file changes or no-op rationale, and no merge/worktree bookkeeping contradiction?
