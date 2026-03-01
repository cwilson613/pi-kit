---
description: Check authentication status across all development tools and refresh expired sessions
---
# Identity Check

Check authentication status across all development tools and refresh expired sessions.

## Process

Run checks:

```bash
# Git identity
echo "Git: $(git config user.name) <$(git config user.email)>"

# GitHub CLI
gh auth status

# OCI registries
podman login --get-login ghcr.io 2>/dev/null || echo "ghcr.io: not logged in"

# Kubernetes
kubectl config current-context 2>/dev/null || echo "k8s: no context"

# AWS
aws sts get-caller-identity 2>/dev/null || echo "AWS: not authenticated"
```

## Status Indicators

| Symbol | Meaning | Action |
|--------|---------|--------|
| `✓` | Authenticated | None |
| `⚠` | Expired | Offer refresh |
| `✗` | Not authenticated | Offer login |

## Refresh Commands

| Domain | Command |
|--------|---------|
| GitHub | `gh auth login` |
| ghcr.io | `gh auth token \| podman login ghcr.io -u $(gh api user --jq .login) --password-stdin` |
| ECR | `aws ecr get-login-password \| podman login <account>.dkr.ecr.<region>.amazonaws.com` |
| AWS SSO | `aws sso login --profile <profile>` |
| K8s | `kubectl config current-context` |
| npm | `npm login` |

For each domain showing issues, offer to run the appropriate refresh command.
