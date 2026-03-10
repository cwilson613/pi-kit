---
id: cleave-title-progress-sync
title: Keep terminal title cleave progress in sync
status: implemented
parent: harness-upstream-error-recovery
tags: [cleave, dashboard, terminal-title, ux]
open_questions: []
---

# Keep terminal title cleave progress in sync

## Overview

Ensure terminal tab titles update as cleave child progress changes so counts like 0/3, 1/3, and 3/3 reflect live dispatcher state instead of only lifecycle phase boundaries.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `extensions/cleave/dispatcher.ts` (modified) — Post-assess reconciliation delta — touched during follow-up fixes
- `extensions/cleave/dispatcher.test.ts` (modified) — Post-assess reconciliation delta — touched during follow-up fixes

### Constraints

- Terminal-title refresh remains event-driven: live cleave counts update when dispatcher child-progress mutations emit dashboard:update, rather than relying on polling or phase-only refreshes.
