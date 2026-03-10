---
id: dashboard-wide-truncation
title: Dashboard wide layout and intelligent truncation
status: implemented
parent: unified-dashboard
tags: [dashboard, ux, truncation, overlay]
open_questions: []
---

# Dashboard wide layout and intelligent truncation

## Overview

Fix dashboard formatting when strings get long by truncating intelligently instead of clipping critical semantics, and expand the dashboard overlay/panel to use a screen-wide blocking inspection layout.

## Open Questions

*No open questions.*

## Implementation Notes

### File Scope

- `extensions/dashboard/footer.ts` (modified) — priority-aware truncation for compact and raised dashboard rows
- `extensions/dashboard/overlay.ts` (modified) — wide centered inspection overlay plus row truncation for nested metadata
- `extensions/dashboard/footer-raised.test.ts` (modified) — regression coverage for raised-row truncation behavior
- `extensions/dashboard/footer-compact.test.ts` (modified) — regression coverage for compact dashboard truncation priorities
- `extensions/dashboard/overlay.test.ts` (modified) — inspection overlay layout coverage
- `extensions/dashboard/overlay-data.test.ts` (modified) — overlay item rendering and truncation coverage
- `docs/dashboard-wide-truncation.md` (modified) — Post-assess reconciliation delta — touched during follow-up fixes

### Constraints

- Dashboard truncation must preserve the leading status/icon and primary label before truncating lower-priority metadata.
- Interactive dashboard inspection view uses a wide centered blocking overlay while the non-capturing panel remains compact.
