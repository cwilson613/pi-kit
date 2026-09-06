# Recovery boundary and unresolved design

Evidence: `/tmp/omegon-dual-link-01.log`, with retained copy under the sibling
`omegon-dual-evidence-01/` directory. The failing owner is
`MaintenanceStateV1::bootstrap`; the installation record compares the full
`PathIdentityV1` against the opened home descriptor.

The installed home path is `/Users/wilson/.omegon`, inode 979551. The stored and
observed device numbers differ. A matching path/inode alone does not establish
that every stored authority remains valid. The diagnostic is observed; a reboot,
remount, filesystem presentation change or directory replacement is not proven.

Resolve before implementation:

- Which stable evidence establishes continuity on supported platforms?
- Which contribution keys, session locks and audit records embed the previous
  identity and must participate in the transaction?
- Can recovery run while existing sessions hold the old identity, or must the
  maintenance owner establish quiescence first?

Use the existing maintenance protocol, locking and audit owners. Preserve an
immutable pre-recovery record and make mapping/settlement explicit. Normal startup
must continue to reject unverified identity changes. Avoid an implicit fallback
that erases deny records or invents new authority when old state is present.
