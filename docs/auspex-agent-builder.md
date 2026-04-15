# Auspex Agent Builder — Design Spec

## Problem

Operators know omegon as an interactive TUI coding assistant. They've used it, they trust its capabilities. Now they want to deploy autonomous agents — long-running daemons that monitor clusters, triage alerts, review PRs, manage infrastructure — using the same toolset. The gap isn't capability (omegon can do all of this) but **the path from "I used it interactively" to "I have a fleet of specialized agents running in production."**

Today that path requires:
1. Understanding the agent manifest format
2. Writing a persona directive from scratch
3. Knowing which domain to pick
4. Manually installing extensions
5. Understanding trigger configs
6. Writing workflow templates
7. Building and pushing OCI images

That's a cliff, not a curve.

## Solution: Agent Builder in Auspex

Auspex provides a guided builder that turns interactive experience into deployable agent bundles. The operator describes what they want in natural language; Auspex generates the bundle, validates it, and deploys it.

### The Builder Flow

```
Operator: "I need an agent that monitors our k8s clusters,
           checks certificate expiry daily, and alerts in Slack
           when pods are crashlooping."

Auspex Agent Builder:
  1. Infers domain → infra (kubectl, helm, ssh, net-diag)
  2. Generates persona directive from description
  3. Creates trigger configs:
     - daily-cert-check (schedule: daily)
     - crashloop-monitor (interval: 5m)
  4. Declares extensions: vox (for Slack alerts)
  5. Sets model/thinking defaults for ops work
  6. Generates agent.toml + PERSONA.md + triggers/
  7. Runs bundle_verify screening
  8. Operator reviews, edits, approves
  9. Auspex deploys: selects OCI image, mounts secrets, creates pod
```

### Builder Inputs

The builder needs three things from the operator:

1. **Intent** — what should this agent do? (natural language)
2. **Integrations** — where does it communicate? (Slack, Discord, webhook)
3. **Credentials** — which k8s secrets to mount

Everything else is inferred or has sensible defaults.

### Builder Outputs

A complete agent bundle directory:

```
generated-agent/
├── agent.toml          # Full manifest
├── PERSONA.md          # Generated from intent
├── AGENTS.md           # Workspace directives
├── mind/
│   └── facts.jsonl     # Domain-specific seed knowledge
├── triggers/
│   ├── trigger-1.toml
│   └── trigger-2.toml
└── verified.json       # Bundle verification stamp
```

Plus a deployment spec (the Auspex spawn contract):

```json
{
  "agent_id": "org.cluster-monitor",
  "image": "ghcr.io/styrene-lab/omegon-infra:0.15.24",
  "command": ["omegon", "serve", "--agent", "org.cluster-monitor"],
  "secrets": {
    "ANTHROPIC_API_KEY": {"from": "k8s:omegon-secrets/anthropic-api-key"},
    "VOX_SLACK_BOT_TOKEN": {"from": "k8s:omegon-secrets/slack-bot-token"}
  },
  "resources": {
    "memory": "1Gi",
    "cpu": "1"
  },
  "probes": {
    "liveness": "/api/healthz",
    "readiness": "/api/readyz"
  }
}
```

## Auspex Spawn Contract

The contract between Auspex and omegon. Auspex reads a resolved agent manifest and produces a pod spec.

### Contract Fields

| Field | Source | Required |
|---|---|---|
| `image` | `agent.domain` → OCI image mapping | Yes |
| `command` | `["omegon", "serve", "--agent", agent.id]` | Yes |
| `port` | Always 7842 | Yes |
| `secrets` | `agent.secrets.required` + `agent.secrets.optional` | Yes |
| `probes.liveness` | `/api/healthz` | Yes |
| `probes.readiness` | `/api/readyz` | Yes |
| `resources.memory` | Domain default (chat=256Mi, coding=512Mi, infra=1Gi) | Yes |
| `resources.cpu` | Domain default (chat=250m, coding=500m, infra=1) | Yes |
| `volumes.omegon_home` | emptyDir or PVC for `$OMEGON_HOME` | Yes |
| `init_containers` | Extension installer (if extensions declared) | If extensions |
| `env.OMEGON_HOME` | `/data/omegon` | Yes |
| `env.RUST_LOG` | `info` (or operator override) | Yes |

### Domain → Image Mapping

| Domain | Image | Default Resources |
|---|---|---|
| `chat` | `ghcr.io/styrene-lab/omegon-chat` | 256Mi / 250m |
| `coding` | `ghcr.io/styrene-lab/omegon` | 512Mi / 500m |
| `coding-python` | `ghcr.io/styrene-lab/omegon-coding-python` | 1Gi / 500m |
| `coding-node` | `ghcr.io/styrene-lab/omegon-coding-node` | 1Gi / 500m |
| `coding-rust` | `ghcr.io/styrene-lab/omegon-coding-rust` | 1Gi / 1 |
| `infra` | `ghcr.io/styrene-lab/omegon-infra` | 1Gi / 1 |
| `full` | `ghcr.io/styrene-lab/omegon-full` | 2Gi / 2 |

### Extension Resolution

Auspex resolves extension dependencies before creating the pod:

1. Read `agent.extensions[]` from manifest
2. For each extension:
   - Look up in extension registry (future: `ghcr.io/styrene-lab/omegon-ext-{name}`)
   - Resolve version constraint against available tags
   - Add to init-container install script
3. Init-container runs before omegon starts:
   ```bash
   # For each extension:
   mkdir -p /data/omegon/extensions/{name}
   curl -fsSL {registry}/{name}/v{version}/manifest.toml > /data/omegon/extensions/{name}/manifest.toml
   curl -fsSL {registry}/{name}/v{version}/{binary} > /data/omegon/extensions/{name}/{binary}
   chmod +x /data/omegon/extensions/{name}/{binary}
   ```

### Lifecycle Management

Auspex manages agent lifecycle:

- **Spawn** — create pod from manifest
- **Health** — poll `/api/healthz` and `/api/readyz`
- **Upgrade** — rolling update with new image tag
- **Scale** — adjust replicas (session router handles multi-caller)
- **Drain** — SIGTERM → graceful shutdown → session save
- **Logs** — stream from pod stdout/stderr
- **Observe** — future: prometheus metrics endpoint on omegon

## Extension Registry (Future)

The missing piece for fully automated extension installation. Not in scope for initial builder, but the manifest format is ready for it.

```
Registry structure:
  ghcr.io/styrene-lab/omegon-ext-vox:0.3.0
  ghcr.io/styrene-lab/omegon-ext-scribe:0.1.0

Each image contains:
  /extension/manifest.toml
  /extension/target/release/{binary}
```

For now, extensions are pre-installed via init-container scripts or baked into custom images. The manifest's `[[extensions]]` declarations serve as documentation and pre-flight validation.

## Community Catalog

Agent bundles are shared via the catalog at `$OMEGON_HOME/catalog/` or a remote registry.

### Authoring a Bundle

```bash
# 1. Create bundle directory
mkdir -p my-agent/mind

# 2. Write manifest
cat > my-agent/agent.toml << 'EOF'
[agent]
id = "myorg.my-agent"
name = "My Agent"
version = "1.0.0"
domain = "coding"
...
EOF

# 3. Write persona
cat > my-agent/PERSONA.md << 'EOF'
# My Agent
You are a specialized agent for...
EOF

# 4. Verify
python3 scripts/bundle_sign.py verify my-agent/

# 5. Generate SBOM
python3 scripts/bundle_sign.py sbom my-agent/

# 6. Test locally
omegon serve --agent ./my-agent

# 7. Submit PR to catalog/
cp -r my-agent catalog/myorg.my-agent
git add catalog/myorg.my-agent && git commit
```

### Testing a Bundle Locally

```bash
# Run with the agent manifest
omegon serve --agent ./catalog/styrene.infra-engineer

# Expected startup log:
# INFO loaded agent manifest agent=styrene.infra-engineer domain=infra
# INFO agent bundle verified
# INFO materialized bundle persona persona="Infrastructure Engineer"
# INFO extension installed extension=vox version=>=0.3.0
# INFO loaded trigger config name=daily-cluster-health schedule=Some("daily")
# INFO daemon dispatch loop started
```

## Two Paths, One Runtime

```
Interactive (Operator)              Deterministic (Auspex)
─────────────────────               ──────────────────────
omegon interactive                  omegon serve --agent X
    │                                   │
    ├─ /login (browser OAuth)           ├─ env: ANTHROPIC_API_KEY
    ├─ /persona (pick from list)        ├─ agent.toml persona section
    ├─ omegon extension install         ├─ init-container installs exts
    ├─ omegon plugin install            ├─ bundle materializes plugins
    ├─ manual config                    ├─ agent.toml settings section
    │                                   │
    ▼                                   ▼
    Same runtime: EventBus, r#loop::run, LlmBridge,
    ContextManager, SessionRouter, triggers, extensions
```

The interactive path is how operators learn and experiment. The deterministic path is how they deploy to production. Same engine, different onramp.
