+++
id = "28fb652e-3d35-4228-8345-8f0ec8f9b0c6"
tags = []
aliases = []
imported_reference = false

[publication]
enabled = false
visibility = "private"
+++

# Cachix Binary Cache

Nix users currently build omegon from source (~15min first build). A Cachix binary cache makes `nix profile install` a download instead of a compile.

## Setup Steps

### 1. Create the cache

```bash
# Install cachix CLI
nix profile install nixpkgs#cachix

# Create the cache (one-time, needs Cachix account)
cachix create styrene
```

This generates a signing keypair. The public key goes in `flake.nix`, the secret key goes in CI.

### 2. Update flake.nix

Re-add the nixConfig block with the correct public key from the cache creation output:

```nix
nixConfig = {
  extra-substituters = [ "https://styrene.cachix.org" ];
  extra-trusted-public-keys = [
    "styrene.cachix.org-1:<actual-key-from-cachix-create>"
  ];
};
```

### 3. Push from CI

Add a step to the release workflow (after build succeeds):

```yaml
# .github/workflows/release.yml — add after the build job
push-cachix:
  needs: build
  runs-on: ubuntu-latest
  if: startsWith(github.ref, 'refs/tags/v')
  steps:
    - uses: actions/checkout@v4
    - uses: cachix/install-nix-action@v30
      with:
        nix_path: nixpkgs=channel:nixos-unstable
    - uses: cachix/cachix-action@v15
      with:
        name: styrene
        authToken: ${{ secrets.CACHIX_AUTH_TOKEN }}
    - run: |
        nix build .#omegon
        cachix push styrene result
```

### 4. Store the auth token

```bash
# Get the token from cachix.org dashboard → cache settings → auth tokens
gh secret set CACHIX_AUTH_TOKEN --body "<token>"
```

### 5. Optional: Push on every main commit

For dev iteration, push from the nightly workflow too:

```yaml
# .github/workflows/nightly.yml — add after version stamp
- uses: cachix/cachix-action@v15
  with:
    name: styrene
    authToken: ${{ secrets.CACHIX_AUTH_TOKEN }}
- run: nix build .#omegon && cachix push styrene result
```

### 6. Multi-arch

The cache is content-addressed — push from both x86_64 and aarch64 runners to cache both architectures:

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, ubuntu-24.04-arm]
```

## Result

After setup, NixOS users get:

```bash
# Binary download (~30s) instead of source build (~15min)
nix profile install github:styrene-lab/omegon
```

The cache is populated automatically by CI on every release tag (and optionally on nightly/main commits).

## Cost

Cachix free tier: 5GB storage, unlimited bandwidth. More than enough for a single binary across two architectures.
