# Plugin loader — TOML manifest discovery, HTTP-backed tools and context

## Intent

Implement the plugin loader that reads ~/.omegon/plugins/*/plugin.toml manifests, creates ToolAdapter instances backed by HTTP endpoints, injects context from declared endpoints, and forwards agent events. This is the extension API contract for all external integrations.

See [design doc](../../../docs/plugin-loader.md).
