+++
id = "3cf24d87-61be-4b10-a780-eb6365883478"
tags = []
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Styrene Identity as operator credential root — RNS identity for secret unlocking and trust — Tasks

## 1. Encrypted secrets.db store
<!-- specs: secrets/store -->

- [x] 1.1 SQLite store at `~/.config/omegon/secrets.db`, WAL mode, never in git (`omegon-secrets/src/store.rs`)
- [x] 1.2 Per-secret AES-256-GCM encryption at rest
- [x] 1.3 Store-level unit tests (13 in store.rs)

## 2. Encryption backends

- [x] 2.1 OS keyring backend (default) — store key via `keyring_set("sh.styrene.omegon", "store-key")`
- [x] 2.2 Passphrase backend — AES key derived via Argon2id (`argon2` 0.5)

## 3. Styrene Identity backend

The previously blocked identity dependency is now published as
`styrene-identity` 0.3.2. Store integration remains opt-in so default builds do
not acquire an identity dependency or prompt for identity access.

- [x] 3.1 Styrene Identity backend — domain-separated HKDF-SHA256 key from `RootSecret`, behind the `styrene-identity` cargo feature
- [ ] 3.2 Backend selection/fallback order: identity (if feature + identity present) → keyring → passphrase prompt

## 4. Deferred to post-0.27.0 — Mesh secrets

- [ ] 4.1 Mesh secret lookups resolve live against the RNS mesh — no local caching of mesh-delivered values
- [ ] 4.2 Trust decisions keyed to RNS identity fingerprints

> Implementation note (2026-06-12): Groups 1 and 2 shipped in the
> omegon-secrets crate. The Styrene Identity backend (group 3) and mesh
> lookups (group 4) are blocked on the RNS identity stack being available
> as a dependency — substantial feature work, not bookkeeping. Original
> scaffolder one-liners replaced with the actual task breakdown.
>
> Implementation note (2026-08-01): Group 3.1 is now implemented using the
> published `styrene-identity` 0.3.2 crate. Automatic backend selection remains
> separate work because it requires an explicit non-interactive identity-unlock
> contract; mesh lookup and trust transport remain deferred.
