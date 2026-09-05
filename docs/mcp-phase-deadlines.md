+++
title = "MCP phase deadlines"
kind = "document"
status = "active"
tags = ["mcp", "configuration"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# MCP phase deadlines

Set operation budgets on a server in `.omegon/mcp.toml`:

```toml
[servers.reference]
command = "reference-mcp"
timeout_secs = 30
startup_timeout_secs = 10
catalog_timeout_secs = 20
execution_timeout_secs = 180
```

Replace `reference-mcp` with your server executable. The fields also apply to
HTTP servers and MCP declarations carried by plugin configuration.

| Setting | Operation bounded |
|---|---|
| `startup_timeout_secs` | Transport connection and initialization. |
| `catalog_timeout_secs` | Complete tool, resource, template, and prompt discovery, including every page. |
| `execution_timeout_secs` | Tool calls, resource reads, and prompt retrieval, including request enqueue and response. |

Explicit phase values must be positive representable durations. An unset phase
inherits `timeout_secs`, whose default remains 30 seconds. Null optional values
in JSON are treated as unset.

For compatibility, servers with neither startup nor catalog overrides keep the
existing shared readiness deadline. Setting either override enables separate
startup and catalog deadlines. Setting only an execution override does not change
readiness behavior. Managed lifecycle admission may impose an outer deadline.

Legacy `timeout_secs = 0` retains its prior special behavior: an internal
one-second readiness allowance, immediate execution timeout, and a one-millisecond
managed outer readiness budget. Pkl now accepts this existing Rust configuration.
Prefer positive explicit phase values for new configurations.

Progress notifications do not extend execution. A cancellation or execution
timeout attempts a request-scoped cancellation notification with a 100-millisecond
notification budget. It does not close the transport or stop unrelated calls.
Diagnostics distinguish notification delivery from remote termination: sending
cancellation does not prove that remote work stopped.

Stalled initialization fails server readiness. With explicit startup/catalog
budgets, stalled catalog discovery also fails readiness. Legacy configuration
retains completed inventory if optional discovery stalls after tools are loaded.
Unsupported optional catalog methods remain nonfatal. Shutdown releases the
client registry before awaiting service settlement, so calls do not wait behind
cleanup to discover that the server is unavailable. Unix local lifecycle cleanup
uses the existing process-group owner to terminate descendants. Windows-host,
container-internal, and mesh-remote process termination require their own runtime
evidence and are not established by local Unix cleanup tests.
