# Recover an installation after its home filesystem identity changes

## Intent

The dual-TUI installation built and installed its binary/launcher pair, then
`catalog install --offline` stopped because the existing maintenance state names
device 16777231 while the current home directory reports 16777233. Its path and
inode still match. The cause of the device change is not established.

Provide an explicit recovery path that preserves contribution deny policy,
session authority and audit history. Do not repair this by deleting maintenance
state or replacing the stored device field in place.

## Scope

Investigate the identity change, define sufficient evidence for rebinding, and
implement a locked, recoverable maintenance transaction. This change is planned;
no live authority migration has been performed.

## Success criteria

- Diagnostics distinguish an identity mismatch from corrupt state.
- Recovery preserves all applicable policy and audit evidence.
- Interrupted recovery has a deterministic safe continuation or rollback.
- The installed home supports catalog and session admission after recovery.
