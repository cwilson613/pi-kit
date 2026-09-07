# Verification

The initial regression failed because HarnessStatus omitted contribution health:
`/tmp/omegon-recovery-ui-red.log` (expected an explicit empty collection, got no
field). The final focused gate `cargo test -p omegon contribution_health --locked`
passed eight tests: `/tmp/omegon-recovery-ui-green.log`.

The fixtures cover absent skill directories, malformed project maintenance state
with a successful independent user scope, reload recovery, an actual legacy home
device mismatch during extension discovery, malformed extension metadata, and
recovery to an absent extension directory. Shared status tests check the typed
error code, root path, cause chain, and Markdown projection. Notice tests check
single aggregate reporting, duplicate suppression, and recovery. Oversized and
multibyte error messages remain bounded while preserving the root cause.

Per-entry plugin and extension failures now remain visible alongside successful
scope counts. Single-extension replacement updates only that entry's health;
full rediscovery replaces the kind's scope and entry results. No guard is bypassed
or cleared by these diagnostics.

Root landing gates and installed runtime evidence remain pending. The only warning
in the focused test log is the existing macOS linker unwind-size warning (plus the
dependency future-incompatibility notice).
