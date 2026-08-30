+++
id = "91d53704-ad57-44b6-a63b-c7381903329e"
tags = []
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Core Component Policy

Core component policy controls whether a packaged Omegon product component can start. The initial disableable component is `core:codescan`.

## Profile Policy

Add `components` to the selected project or user profile JSON:

```json
{
  "components": {
    "core:codescan": { "enabled": false }
  }
}
```

An omitted entry uses the packaged composition default. An explicit `enabled: true` request cannot override a user-local or propagated deny.

Use the shared commands to inspect or change the selected profile source:

```text
/profile components view
/profile component disable core:codescan
/profile component enable core:codescan
```

Changes apply to the next process boot. They do not mutate the component generation captured by an active session.

## User-Local Deny

`OMEGON_HOME/component-policy.json` supplies a machine-local deny floor:

```json
{
  "schemaVersion": 1,
  "components": {
    "core:*": { "enabled": false }
  }
}
```

The user-local file is deny-only. Remove a deny entry to remove the floor. `core:*` selects all release-declared disableable core components. It does not select the constitutional kernel, default loop, host effects, or maintenance recovery capability.

Validation is strict. Omegon rejects unknown fields, non-Boolean values, malformed selectors, unknown exact component IDs, unsupported schema versions, and attempts to disable non-disableable capabilities. The error names the source path and invalid field or selector. Validation finishes before component discovery or process startup.

## Runtime And Migration

A denied component remains packaged but does not start, probe readiness, index files, or mutate its database. Its tools are absent from the model-callable schema. Direct CLI, ACP, and tool calls retain a stable contract and return `service:disabled` with `core:codescan` and the determining profile, user-local, or propagated source.

Legacy profiles that contain `omegon-codescan` in `extensions.disabled` still deny `core:codescan`. The next canonical profile save writes the component entry and removes only that legacy codescan entry. Other extension rules remain unchanged.

`/extension enable|disable` controls operator-managed SDK extension installation state. It does not change release-coupled product component policy. Use `/profile component ...` for `core:codescan`.

## Package And Update Integrity

Disablement changes runtime activation only. Release archives, signed package manifests, member digests, resident composition locks, complete update generations, and rollback candidates must still contain and validate required codescan files. A missing or corrupt required sidecar fails package or update validation even when effective policy denies `core:codescan`.
