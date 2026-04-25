#!/usr/bin/env python3
"""Deploy a static site directory to Cloudflare Pages via the official REST API.

Replaces `npx wrangler pages deploy` — no npm, no wrangler, no Node.js.
Uses the Cloudflare v4 API directly via urllib (stdlib only, no pip deps).

Usage:
    python3 scripts/deploy_cloudflare_pages.py <directory> <project_name>

Environment:
    CLOUDFLARE_ACCOUNT_ID   — account ID (required)
    CLOUDFLARE_API_TOKEN    — API token with Pages:Edit (required)
    CF_BRANCH               — branch name for deployment (default: main)
    CF_COMMIT_HASH          — git commit SHA (optional)
    CF_COMMIT_MESSAGE       — commit message (optional)
"""

import hashlib
import json
import mimetypes
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path


API_BASE = "https://api.cloudflare.com/client/v4"


def die(msg: str) -> None:
    print(f"✗ {msg}", file=sys.stderr)
    sys.exit(1)


def api_request(
    method: str, path: str, token: str, data: bytes | None = None,
    content_type: str = "application/json",
) -> dict:
    url = f"{API_BASE}{path}"
    req = urllib.request.Request(url, method=method, data=data)
    req.add_header("Authorization", f"Bearer {token}")
    if data is not None:
        req.add_header("Content-Type", content_type)
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            body = json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body_text = e.read().decode("utf-8", errors="replace")
        die(f"API {method} {path} returned {e.code}: {body_text}")
    except urllib.error.URLError as e:
        die(f"API {method} {path} failed: {e.reason}")
    if not body.get("success"):
        errors = body.get("errors", [])
        die(f"API error: {json.dumps(errors, indent=2)}")
    return body


def hash_file(path: Path) -> str:
    """Cloudflare uses xxhash64, but the manifest accepts SHA-256 hex too."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def collect_files(directory: Path) -> dict[str, Path]:
    """Walk the directory and return {relative_path: absolute_path}."""
    files = {}
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            rel = "/" + str(path.relative_to(directory))
            files[rel] = path
    return files


def create_multipart_body(
    manifest: dict[str, str],
    files: dict[str, Path],
    branch: str,
    commit_hash: str | None,
    commit_message: str | None,
) -> tuple[bytes, str]:
    """Build a multipart/form-data body for the deployment create endpoint."""
    boundary = "----OmegonDeployBoundary"
    parts = []

    def add_field(name: str, value: str) -> None:
        parts.append(
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="{name}"\r\n'
            f"\r\n"
            f"{value}\r\n"
        )

    # Manifest: JSON mapping path → hash
    add_field("manifest", json.dumps(manifest))
    add_field("branch", branch)
    if commit_hash:
        add_field("commit_hash", commit_hash)
    if commit_message:
        add_field("commit_message", commit_message)

    # File contents as named parts
    for rel_path, abs_path in files.items():
        mime = mimetypes.guess_type(str(abs_path))[0] or "application/octet-stream"
        file_data = abs_path.read_bytes()
        file_hash = manifest.get(rel_path, rel_path)
        parts.append(
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="{file_hash}"; filename="{rel_path}"\r\n'
            f"Content-Type: {mime}\r\n"
            f"\r\n"
        )
        parts.append(file_data)
        parts.append(b"\r\n")

    parts.append(f"--{boundary}--\r\n")

    # Combine string and bytes parts
    body = b""
    for part in parts:
        if isinstance(part, str):
            body += part.encode("utf-8")
        else:
            body += part

    content_type = f"multipart/form-data; boundary={boundary}"
    return body, content_type


def ensure_project(account_id: str, project_name: str, token: str) -> None:
    """Create the Pages project if it doesn't exist."""
    path = f"/accounts/{account_id}/pages/projects/{project_name}"
    try:
        api_request("GET", path, token)
    except SystemExit:
        # Project doesn't exist — create it
        print(f"  Creating project '{project_name}'...")
        data = json.dumps({
            "name": project_name,
            "production_branch": "main",
        }).encode()
        api_request(
            "POST",
            f"/accounts/{account_id}/pages/projects",
            token,
            data=data,
        )


def deploy(
    directory: Path,
    project_name: str,
    account_id: str,
    token: str,
    branch: str = "main",
    commit_hash: str | None = None,
    commit_message: str | None = None,
) -> str:
    """Deploy a directory to Cloudflare Pages. Returns the deployment URL."""
    print(f"  Collecting files from {directory}...")
    files = collect_files(directory)
    if not files:
        die(f"No files found in {directory}")
    print(f"  {len(files)} files to deploy")

    # Build manifest
    manifest = {}
    for rel_path, abs_path in files.items():
        manifest[rel_path] = hash_file(abs_path)

    ensure_project(account_id, project_name, token)

    print(f"  Deploying to {project_name} (branch: {branch})...")
    body, content_type = create_multipart_body(
        manifest, files, branch, commit_hash, commit_message,
    )

    path = f"/accounts/{account_id}/pages/projects/{project_name}/deployments"
    result = api_request("POST", path, token, data=body, content_type=content_type)

    deployment = result.get("result", {})
    url = deployment.get("url", "(unknown)")
    deploy_id = deployment.get("id", "(unknown)")
    print(f"  Deployed: {url}")
    print(f"  ID: {deploy_id}")
    return url


def main() -> None:
    if len(sys.argv) < 3:
        print("Usage: deploy_cloudflare_pages.py <directory> <project_name>")
        sys.exit(1)

    directory = Path(sys.argv[1])
    project_name = sys.argv[2]

    if not directory.is_dir():
        die(f"Not a directory: {directory}")

    account_id = os.environ.get("CLOUDFLARE_ACCOUNT_ID", "")
    token = os.environ.get("CLOUDFLARE_API_TOKEN", "")

    if not account_id:
        die("CLOUDFLARE_ACCOUNT_ID not set")
    if not token:
        die("CLOUDFLARE_API_TOKEN not set")

    branch = os.environ.get("CF_BRANCH", "main")
    commit_hash = os.environ.get("CF_COMMIT_HASH")
    commit_message = os.environ.get("CF_COMMIT_MESSAGE")

    deploy(directory, project_name, account_id, token, branch, commit_hash, commit_message)


if __name__ == "__main__":
    main()
