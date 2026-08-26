+++
id = "de58cc7c-2ba8-4a88-9863-bdb866164af3"
tags = ["armory", "discovery", "extensions", "skills", "plugins"]
aliases = ["armory-discovery"]
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Armory Discovery

Omegon treats `styrene-lab/omegon-armory` as the upstream discovery catalog for ecosystem assets.

## Browse Surfaces

CLI:

```sh
omegon armory browse
omegon armory browse --kind extensions browser
omegon armory search security --kind skills
omegon armory browse --kind plugins
omegon armory browse --kind agents --json
```

Slash command:

```text
/armory
/armory browser
/armory search security
```

ACP:

```json
{
  "method": "armory/browse",
  "params": {
    "kind": "skills",
    "query": "security"
  }
}
```

`kind` accepts `all`, `extensions`, `plugins`, `skills`, or `agents`.

## Sources

- `registry.toml` provides installable native extensions.
- `catalog-registry.toml` provides catalog agent bundles.
- `personas/*/plugin.toml`, `tones/*/plugin.toml`, `skills/*/plugin.toml`, and `examples/*/plugin.toml` provide Armory plugin and skill metadata.

The browse response includes `kind`, `id`, `name`, `description`, `category`, `version`, `source`, `manifest_id`, `installed`, and `install_hint`.

## Installed State

Installed status is computed locally:

- Extensions: `~/.omegon/extensions/<name>/`
- Agents: `~/.omegon/catalog/<agent-id>/agent.toml`
- Skills: `~/.omegon/skills/<slug>/SKILL.md`, `.omegon/skills/<slug>/SKILL.md`, or an Armory skill plugin
- Plugins: `~/.omegon/plugins/<slug>/plugin.toml`, `.omegon/plugins/<slug>/plugin.toml`, or `~/.omegon/armory/<root>/<slug>/plugin.toml`

This makes the discovery model reusable by terminal UX, Auspex/Flynt UI surfaces, and ACP clients without each caller reimplementing Armory registry parsing.

## Admission and lifecycle

Browse and installed state are inventory, not execution authority. An executable Armory bundle uses `plugin:<directory-name>` and must be listed in `permissions.trustedContributionCode` before Omegon evaluates Pkl, generates dynamic context, runs scripts or OCI tools, sends HTTP requests, or connects plugin MCP services. The host binds that admission to the current bundle snapshot.

Armory context readiness is bounded. Owned script and OCI process groups are killed and reaped on timeout or cancellation, while the contribution boundary reports best-effort cleanup rather than claiming verified confinement. Armory publishes zero automatic restarts and quarantine-on-failure lifecycle policy. `/status` reports accepted composition state, contribution health, cleanup assurance, and coded graph diagnostics.
