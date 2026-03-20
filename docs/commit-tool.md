---
id: commit-tool
title: Structured commit tool — replaces bash git commit
status: implemented
parent: git-harness-integration
tags: [rust, git, tools]
open_questions: []
---

# Structured commit tool — replaces bash git commit

## Overview

A first-class agent tool that replaces git commit via bash. Takes a message and optional scope. Consults RepoModel for dirty files, handles submodule two-level commits automatically, folds in pending lifecycle changes, and applies commit policy (conventional commit format validation). The agent calls commit(message) instead of bash(git add -A && git commit -m ...).

## Open Questions

*No open questions.*
