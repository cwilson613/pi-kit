---
id: update-check-restart
title: Signed update check with in-app restart
status: exploring
tags: [release, ux, lifecycle]
open_questions:
  - Where do we check for updates — GitHub Releases API, a custom endpoint, or both?
  - How do we verify the downloaded binary — cosign signature, minisign, or ad-hoc codesign?
jj_change_id: lszorvvykkkmktrvtsnkvtmzoruzmmss
---

# Signed update check with in-app restart

## Overview

Check for new versions of the omegon binary at startup and on-demand. When a signed update is available, display a notification in the TUI. Support in-app restart: download the new binary, verify signature, replace the running binary, and exec() into the new version preserving the session.

## Open Questions

- Where do we check for updates — GitHub Releases API, a custom endpoint, or both?
- How do we verify the downloaded binary — cosign signature, minisign, or ad-hoc codesign?
