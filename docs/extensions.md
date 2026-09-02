---
id: extensions-runtime-guide
tags: [extensions, plugins, mcp, security, lifecycle]
aliases: [extension-authoring, contribution-runtime]
imported_reference: false

publication:
  enabled: false
  visibility: private
---

# Dynamic contribution runtime guide

This is the canonical Omegon host-runtime guide for extension, executable plugin, and MCP admission. The standalone [`omegon-extension-rs`](https://github.com/styrene-lab/omegon-extension-rs) repository owns the Rust SDK and JSON-RPC API. Host installation, trust, composition, lifecycle, and diagnostics are owned here.

## Identities

Omegon assigns stable contribution IDs before executing dynamic code:

| Contribution | Stable ID |
|---|---|
| Native or OCI extension | `extension:<manifest extension.name>` |
| Executable plugin bundle | `plugin:<plugin directory name>` |
| Project `.omegon/mcp.toml` | `mcp:project` |
| ACP-supplied MCP configuration | `mcp:acp-client` |

The plugin ID comes from the admitted directory name, not `[plugin].name`. Changing a display name does not change execution authority.

## Installation is not trust

Installation, enablement, activation markers, extension selection, maintenance admission, and `trustedDirectories` do not authorize dynamic code. Explicitly trust reviewed stable IDs in a user or project profile:

```json
{
  "permissions": {
    "trustedContributionCode": [
      "extension:my-extension",
      "plugin:my-plugin",
      "mcp:project"
    ]
  }
}
```

This grants trusted host execution to those identities. It does not grant tool capabilities, approve individual effects, or establish a sandbox. `--dangerously-bypass-permissions` bypasses interactive tool/filesystem mediation only; it does not mint trusted-contribution or verified-confinement evidence.

## Source snapshots and review

Before evaluation, spawn, secret expansion, or connection, the host constructs a non-executable preflight with the stable ID, source kind, requested effects, protocol range, readiness budget, and a digest of the admitted source snapshot. The runtime permit binds the stable ID to those exact bytes and is revalidated at deferred execution and respawn boundaries.

The profile entry remains identity-based across updates; it does not permanently pin one digest. Re-review updates before installing them because newly installed bytes can receive a new source-bound runtime permit under an already trusted ID. Local installation copies the bundle into Omegon's guarded extension root. Rebuilding the original source directory does not update that installed copy. Rebuild and reinstall or update, then run `/extension refresh` while the session is idle. Compatible EventBus and native-RPC generations publish at explicit quiescence. Widget or voice side-channel changes still require `/runtime restart`.

## Trust is not confinement

Native extensions, scripts, local MCP processes, and current OCI paths can exercise host authority. Process or container separation provides crash isolation, not verified confinement. Omegon claims verified confinement only when an enforced boundary blocks direct filesystem, process, network, and secret access and routes privileged effects through host brokers. No currently supported dynamic extension/plugin/MCP path supplies that evidence.

Manifest trust or confinement requests cannot grant either property. A profile that requires strict cleanup rejects a transport whose complete resource tree Omegon cannot own.

## Readiness and publication

Native extensions, MCP process and HTTP servers, and executable manifest HTTP, script, and OCI adapters enter one metadata-only candidate inventory. Discovery captures stable identity, source kind, source digest, trust and confinement requests, and probe requirements. It does not evaluate Pkl, spawn a process or container, connect to a service, resolve secrets, or publish registrations.

After trust admission, candidates are quarantined from ordinary dispatch while their transport adapters negotiate declarations. EventBus publishes a new composition generation only after the complete graph passes validation, readiness, and compatibility-cache parity. One generation owner performs rollback and shutdown for every adapter. A failed candidate is cleaned up and cannot replace the previously accepted graph. Process adapters retain process-tree cleanup. HTTP adapters do not claim that the remote peer settled.

Changed native extension bytes use one hidden pending generation per contribution. A newer candidate settles the older pending process before it becomes the sole candidate. `/extension refresh` publishes only through the supervisor-owned quiescent transaction. Active turns, queued work, unknown invocation authority, or active extension calls prevent commit. Retained leases and polling handles consult the shared generation fence and fail before old-generation RPC owner entry after publication.

| Adapter | Readiness and failure policy | Cleanup assurance |
|---|---|---|
| Extension | One manifest `startup.timeout_ms` deadline spans initialization, tool discovery, configuration, and secret delivery. Transport failures use a fixed generation-local restart budget with capped backoff, then terminal quarantine. | Strict only for host-owned Unix native process groups; otherwise best effort. |
| Project or ACP MCP | One per-server deadline spans connection, required tool discovery, and optional resource/template/prompt discovery. Slice 2 does not automatically restart MCP services. | Best effort; remote services cannot claim strict host cleanup. |
| Armory context/script/OCI | Context readiness is bounded. Script and OCI process groups are killed and reaped on timeout or cancellation. Script paths must remain within the admitted snapshot. | Best effort at the contribution boundary. |
| HTTP plugin | Requests use bounded HTTP timeouts and failures degrade locally. | Best effort. |

A changed extension generation receives a fresh restart controller. A successful respawn within one generation does not erase earlier crash evidence.

## Invocation and host effects

Extension tools execute through the shared generation-bound invocation path. Owner acknowledgement and terminal settlement become durable before ordinary completion is published; ambiguous transport loss after dispatch becomes unknown completion rather than an automatic replay.

Generic ACP extension RPC uses one conservative extension-owned Operator/ACP transport capability because the current protocol does not declare effects per method. Dispatch runs on the worker-owned EventBus and does not infer safety from method names. Per-method read-only, mutating, host-action-mediated, or denied declarations remain future work.

An extension cannot bypass admission by asking the host to execute an imperative action directly. Lease-less `actions/execute` requests fail closed. Declarative native and MCP HostActions require a live dispatching parent lease, effects contained by that lease, and an exactly-once child identity. Operator approval contributes intent only; it does not grant missing project policy, runtime policy, trusted-origin, or parent authority.

## Diagnostics

Native and ACP `/status` consume one semantic composition projection. It identifies the active `composition:<opaque-id>`, effective contributions, owner tier and contribution generation, current health, cleanup assurance/state, coded candidate diagnostics, and explicit `graph_derived_legacy` dispatch parity. The structured projection also carries negotiated protocols, activation waves, and replacement edges.

`quarantined` means the contribution is not eligible for ordinary activation or further silent respawn. It does not mean its code ran in a security sandbox. Cleanup shown as `best_effort` or `unverified` must not be interpreted as proven process-tree settlement.

## Authoring workflow

1. Scaffold with `omegon extension init <name>` and implement the external SDK's `execute_tool` JSON-RPC contract.
2. Build and install with `omegon extension install .`; local installation copies the candidate bundle.
3. Review the installed code and add `extension:<name>` to `permissions.trustedContributionCode`.
4. Run `/extension refresh` while idle and inspect `/status` for readiness, quarantine, cleanup, or graph diagnostics.
5. After source changes, rebuild and reinstall before refresh; do not assume the original checkout remains linked. Use `/runtime restart` when the extension provides widgets or voice side channels.

See `pkl/ExtensionManifest.pkl`, `pkl/PluginManifest.pkl`, and `pkl/McpConfig.pkl` for configuration shape. These schemas describe requests and configuration, not host trust grants.
