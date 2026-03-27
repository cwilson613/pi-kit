---
id: omegon-design-crate-extraction
title: Extract design tree core into omegon-design crate
status: seed
parent: git-native-task-management
tags: [architecture, crate, design-tree, task-management]
open_questions: []
jj_change_id: urroornuzoyklopmyzxtuytzwknnxtqp
issue_type: feature
priority: 1
---

# Extract design tree core into omegon-design crate

## Overview

Split the reusable design tree core out of omegon's lifecycle module into a standalone crate. Move types, markdown store/parsing, doctor/audit, filtering, history, and index logic into `core/crates/omegon-design/`. Keep agent-specific context injection and tool registration in the omegon binary.

## Open Questions

*No open questions.*
