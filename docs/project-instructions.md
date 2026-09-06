+++
title = "Project instruction loading"
kind = "document"
status = "active"
tags = ["configuration", "instructions"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Project instruction loading

Omegon loads `AGENTS.md` when it constructs the session's base prompt. In a Git
worktree it reads every ancestor from the active worktree root through the current
directory. Each file appears once, with its source path, in root-to-cwd order.
Nearest-scope guidance adds to root policy and cannot override Core Directives.

A linked worktree uses its own files. The `.git` file points to Git storage and
does not redirect instruction discovery to the main checkout. Outside a Git
worktree, discovery checks only the current directory. It does not scan sibling
directories or descendants below cwd.

Explicitly symlinked instruction files remain supported, including shared files
outside the worktree. Canonical paths prevent duplicate content when several
ancestor files link to the same source. A dangling symlink is a read failure.

Files retain their complete UTF-8 content. Missing files are optional. An existing
file that cannot be read, including invalid UTF-8, stops prompt preparation with
the source identified. Repair the file and retry preparation.

Before model dispatch, the loop checks estimated system instructions, tool
schemas, and the selected reply reserve against model capacity. If fixed content
cannot fit, it reports an error without sending a provider request. Reduce the
required content or select a model with sufficient capacity. Estimation uses the
existing token estimator; it is not an exact provider tokenizer.

Global operator guidance keeps its existing loading and mode rules. This change
does not add live refresh or durable instruction generations. Reconstruct the
session prompt to pick up changed project policy; an already constructed prompt
does not automatically reread files on each turn.
