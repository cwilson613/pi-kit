# Vault client fail-closed security hardening

## Intent

Security assessment revealed multiple fail-open defaults in the vault client. The client-side path enforcement uses a deny-list-first model where an empty allowlist permits all paths. Auth failures leave a "configured but unauthenticated" client that downstream code treats as ready. Error paths propagate raw server response bodies. All of these must flip to fail-closed: deny by default, reject ambiguity, surface failures loudly.
