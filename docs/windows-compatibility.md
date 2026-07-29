---
id: windows-compatibility
title: "Windows compatibility and WSL host-boundary constraints"
status: exploring
tags: [windows, wsl, compatibility, community]
open_questions:
  - "[assumption] Community contributors who extend Windows support will have access to representative native Windows and WSL environments for platform-specific integration tests."
  - "[assumption] The documented support contract may remain Linux/macOS-first without requiring warnings on every WSL invocation of a Windows-host executable."
dependencies: []
related: []
---

# Windows compatibility and WSL host-boundary constraints

## Overview

Omegon is Linux/macOS-first. Native Windows support and complete lifecycle control across the WSL-to-Windows interoperability boundary are explicitly non-goals for the core team at present. Installed Omegon binaries running inside WSL use Linux `/bin/bash`, POSIX process groups, and Unix signals; timeout/cancellation cleanup covers Linux descendants. Commands that cross into Windows-host executables (`*.exe`, PowerShell, `cmd.exe`) leave the Linux process-group lifecycle, so descendant cleanup is best-effort and cannot be guaranteed. Workspaces under `/mnt/<drive>` may also inherit Windows filesystem semantics and performance constraints. Future native-Windows or WSL-host interoperability hardening is suitable for community contributions, provided it preserves Linux/macOS behavior and adds platform-specific tests.

## Decisions

### Accept the WSL-to-Windows host process boundary

**Status:** accepted

**Rationale:** The current process-group implementation correctly addresses native Linux descendants under WSL. Guaranteeing cleanup after Windows interoperability dispatch requires Windows job/process-tree machinery and materially expands platform scope.

### Treat extended Windows compatibility as community-owned

**Status:** accepted

**Rationale:** The operator prioritizes Linux/macOS engineering capacity and accepts reduced Windows guarantees. Recording ownership prevents accidental expansion of the core support contract.

## Open Questions

- [assumption] Community contributors who extend Windows support will have access to representative native Windows and WSL environments for platform-specific integration tests.
- [assumption] The documented support contract may remain Linux/macOS-first without requiring warnings on every WSL invocation of a Windows-host executable.
