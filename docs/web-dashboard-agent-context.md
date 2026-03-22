---
id: web-dashboard-agent-context
title: Web dashboard agent context — pass active tab and visible state to the agent with prompts
status: seed
parent: auto-doc-generation
tags: [web, dashboard, ux, agent, context]
open_questions: []
jj_change_id: mupspnvpmvkuvnlnvsymzrtsnzwrpwlm
---

# Web dashboard agent context — pass active tab and visible state to the agent with prompts

## Overview

When the operator sends a message from the web dashboard, the agent receives only the text — no context about what the dashboard is showing. If the user is on the Graph tab looking at the design tree visualization and asks 'what can you tell me about this graph?', the agent has no idea what graph they mean.

The fix: when the web UI sends a user_prompt, include metadata about the active view — which tab is selected, what data is visible (e.g. the list of graph node IDs on screen), and any selection state. The agent can then use this context to provide relevant answers without the user having to explain what they're looking at.

## Open Questions

*No open questions.*
