---
id: session-resume-degradation-visibility
title: "Resume Degradation Visibility"
status: implemented
tags: [session, resume, context, upgrade]
open_questions: []
dependencies: []
related: []
---

# Resume Degradation Visibility

## Overview

This issue described schema-v1 compatibility resume, which keeps a recent tail
and folds older messages into a synthetic summary. Slice 5 now makes the boundary
explicit: full semantic lineage resumes exactly, mixed lineage exposes one
labeled compatibility base plus an exact suffix, and legacy lineage retains the
lossy behavior with no exactness claim. `/transcript` and `/session-export`
separate exact semantic output from presentation/evidence output, while corrupt
required authority fails closed instead of silently degrading to the pair.
