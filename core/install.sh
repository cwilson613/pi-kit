#!/bin/sh
# Install omegon from GitHub Releases.
#
# Usage:
#   curl -fsSL https://omegon.styrene.io/install.sh | \
#     OMEGON_BOOTSTRAP_VERIFIER=/absolute/path/to/trusted/omegon-maintain sh
#
# Non-interactive:
#   curl -fsSL https://omegon.styrene.io/install.sh | \
#     OMEGON_BOOTSTRAP_VERIFIER=/absolute/path/to/trusted/omegon-maintain \
#     sh -s -- --no-confirm
#
# Or directly from GitHub:
#   curl -fsSL https://raw.githubusercontent.com/styrene-lab/omegon/main/core/install.sh | \
#     OMEGON_BOOTSTRAP_VERIFIER=/absolute/path/to/trusted/omegon-maintain sh
#
# Environment variables:
#   INSTALL_DIR   — installation directory (default: /usr/local/bin)
#   VERSION       — specific version to install (default: latest)
#   NO_COLOR      — disable colored output (set to any value)
# An independently acquired and trusted verifier is mandatory:
#   OMEGON_BOOTSTRAP_VERIFIER — absolute path to a trusted verifier implementing
#                               `--json release verify`
# Test-only local release seam (all four variables are required together):
#   OMEGON_INSTALL_ARCHIVE    — absolute path to a release archive
#   OMEGON_INSTALL_CHECKSUMS  — absolute path to its checksums.sha256
#   OMEGON_INSTALL_MANIFEST   — absolute path to its package manifest
#   OMEGON_INSTALL_BUNDLE     — absolute path to its Sigstore bundle
#
# Manual download:
#   https://github.com/styrene-lab/omegon/releases

set -eu

REPO="styrene-lab/omegon"
BINARY="omegon"
MAINTAIN_BINARY="omegon-maintain"
# Default install dir: prefer /usr/local/bin if writable, else ~/.local/bin.
# This avoids requiring sudo on systems where /usr/local/bin isn't writable.
if [ -z "${INSTALL_DIR:-}" ]; then
  if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
fi
VERSION="${VERSION:-}"
CHANNEL="${CHANNEL:-stable}"
GITHUB_API="https://api.github.com/repos/${REPO}"
TMP=""
EXTRACTED_ROOT=""
STAGING_DIR=""
PUBLISHED_CANDIDATE=""
INSTALL_SUCCEEDED=false
NO_CONFIRM=false
RECEIPT_DIR="${HOME}/.config/omegon"
LOCAL_ARCHIVE="${OMEGON_INSTALL_ARCHIVE:-}"
LOCAL_CHECKSUMS="${OMEGON_INSTALL_CHECKSUMS:-}"
LOCAL_MANIFEST="${OMEGON_INSTALL_MANIFEST:-}"
LOCAL_BUNDLE="${OMEGON_INSTALL_BUNDLE:-}"
BOOTSTRAP_VERIFIER="${OMEGON_BOOTSTRAP_VERIFIER:-}"

# ── Parse arguments ───────────────────────────────────────────

for arg in "$@"; do
  case "$arg" in
    --no-confirm) NO_CONFIRM=true ;;
    --channel=*) CHANNEL="${arg#--channel=}" ;;
    --version=*) VERSION="${arg#--version=}" ;;
    --help|-h)
      echo "Usage: curl -fsSL https://omegon.styrene.io/install.sh | OMEGON_BOOTSTRAP_VERIFIER=/absolute/path/to/trusted/omegon-maintain sh"
      echo ""
      echo "Options (pass after 'sh -s --'):"
      echo "  --no-confirm        Skip interactive confirmation"
      echo "  --channel=CHANNEL   Release channel: stable | nightly (default: stable)"
      echo "  --version=VERSION   Pin a specific version tag (default: latest for channel)"
      echo ""
      echo "Environment:"
      echo "  INSTALL_DIR     Installation directory (default: /usr/local/bin)"
      echo "  VERSION         Pin a specific version tag (default: latest for selected channel)"
      echo "  CHANNEL         Release channel: stable | nightly (default: stable)"
      echo "  NO_COLOR        Disable colored output"
      echo "  OMEGON_BOOTSTRAP_VERIFIER"
      echo "                   Absolute path to an independently trusted release verifier (required)"
      exit 0
      ;;
  esac
done

# ── Color support ─────────────────────────────────────────────

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  ESC=$(printf '\033')
  BOLD="${ESC}[1m"
  DIM="${ESC}[2m"
  CYAN="${ESC}[0;36m"
  GREEN="${ESC}[0;32m"
  YELLOW="${ESC}[0;33m"
  RED="${ESC}[0;31m"
  RESET="${ESC}[0m"
else
  BOLD="" DIM="" CYAN="" GREEN="" YELLOW="" RED="" RESET=""
fi

# ── Helpers ───────────────────────────────────────────────────

step()    { printf "${CYAN}  ▸${RESET} %s\n" "$*"; }
ok()      { printf "${GREEN}  ✓${RESET} %s\n" "$*"; }
warn()    { printf "${YELLOW}  ⚠${RESET} %s\n" "$*"; }
err()     { printf "${RED}  ✗${RESET} %s\n" "$*" >&2; }
die()     { err "$*"; cleanup; exit 1; }
dimtext() { printf "${DIM}%s${RESET}" "$*"; }

cleanup() {
  if [ -n "$STAGING_DIR" ] && [ -d "$STAGING_DIR" ]; then
    rm -rf "$STAGING_DIR"
  fi
  if [ "$INSTALL_SUCCEEDED" = false ] && [ -n "$PUBLISHED_CANDIDATE" ] && [ -d "$PUBLISHED_CANDIDATE" ]; then
    rm -rf "$PUBLISHED_CANDIDATE"
  fi
  if [ -n "$TMP" ] && [ -d "$TMP" ]; then
    rm -rf "$TMP"
  fi
}

abort() { cleanup; trap - EXIT; exit 1; }
trap cleanup EXIT
trap abort HUP INT TERM

# ── Preflight checks ─────────────────────────────────────────

case "$BOOTSTRAP_VERIFIER" in
  /*) ;;
  *) die "OMEGON_BOOTSTRAP_VERIFIER must explicitly name an absolute independently trusted verifier" ;;
esac
[ -f "$BOOTSTRAP_VERIFIER" ] && [ ! -L "$BOOTSTRAP_VERIFIER" ] && [ -x "$BOOTSTRAP_VERIFIER" ] ||
  die "OMEGON_BOOTSTRAP_VERIFIER must name an executable regular file, not a symlink"

if [ -n "$LOCAL_ARCHIVE" ] || [ -n "$LOCAL_CHECKSUMS" ] || [ -n "$LOCAL_MANIFEST" ] || [ -n "$LOCAL_BUNDLE" ]; then
  [ -n "$LOCAL_ARCHIVE" ] && [ -n "$LOCAL_CHECKSUMS" ] &&
    [ -n "$LOCAL_MANIFEST" ] && [ -n "$LOCAL_BUNDLE" ] ||
    die "OMEGON_INSTALL_ARCHIVE, OMEGON_INSTALL_CHECKSUMS, OMEGON_INSTALL_MANIFEST, and OMEGON_INSTALL_BUNDLE must be supplied together"
  [ "${LOCAL_ARCHIVE#/}" != "$LOCAL_ARCHIVE" ] && [ -f "$LOCAL_ARCHIVE" ] ||
    die "OMEGON_INSTALL_ARCHIVE must name an absolute local file"
  [ "${LOCAL_CHECKSUMS#/}" != "$LOCAL_CHECKSUMS" ] && [ -f "$LOCAL_CHECKSUMS" ] ||
    die "OMEGON_INSTALL_CHECKSUMS must name an absolute local file"
  [ "${LOCAL_MANIFEST#/}" != "$LOCAL_MANIFEST" ] && [ -f "$LOCAL_MANIFEST" ] ||
    die "OMEGON_INSTALL_MANIFEST must name an absolute local file"
  [ "${LOCAL_BUNDLE#/}" != "$LOCAL_BUNDLE" ] && [ -f "$LOCAL_BUNDLE" ] ||
    die "OMEGON_INSTALL_BUNDLE must name an absolute local file"
else
  command -v curl >/dev/null 2>&1 || die "curl is required but not found"
fi
command -v tar >/dev/null 2>&1 || die "tar is required but not found"

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "sha256sum or shasum is required for checksum verification"
fi

json_string_field() {
  json_file="$1"
  json_key="$2"
  sed -n 's/.*"'"$json_key"'"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$json_file" | head -1
}

json_number_field() {
  json_file="$1"
  json_key="$2"
  sed -n 's/.*"'"$json_key"'"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$json_file" | head -1
}

verification_result_succeeded() {
  result_file="$1"
  [ "$(json_string_field "$result_file" status)" = "success" ] &&
    grep -Eq '"code"[[:space:]]*:[[:space:]]*"release_verified"' "$result_file"
}

validate_resident_lock() {
  generation="$1"
  executable="$2"
  identity="$3"
  lock="${generation}/${executable}.composition-lock.json"
  [ "$(json_number_field "$lock" schema_version)" = "1" ] &&
    [ "$(json_string_field "$lock" executable_identity)" = "$identity" ] &&
    [ "$(json_string_field "$lock" executable_digest)" = "$(sha256 "${generation}/${executable}")" ] &&
    [ "$(json_string_field "$lock" target)" = "$PLATFORM" ] &&
    [ "$(json_number_field "$lock" protocol_minimum)" = "1" ] &&
    [ "$(json_string_field "$lock" issuer)" = "https://token.actions.githubusercontent.com" ] &&
    [ "$(json_string_field "$lock" verification)" = "required" ] ||
    die "resident composition lock is invalid for ${identity}"
  case "$(json_string_field "$lock" workflow_identity)" in
    https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v*) ;;
    *) die "resident composition lock signing identity is invalid for ${identity}" ;;
  esac
}

validate_product_component() {
  generation="$1"
  lock="${generation}/${COMPONENT_LOCK_RELATIVE}"
  manifest="${generation}/${CODESCAN_RELATIVE}/manifest.toml"
  executable="${generation}/${CODESCAN_RELATIVE}/target/release/omegon-codescan"
  [ "$(json_number_field "$lock" schema_version)" = "1" ] &&
    [ "$(json_string_field "$lock" component_id)" = "core:codescan" ] &&
    [ "$(json_string_field "$lock" wire_manifest_id)" = "omegon-codescan" ] &&
    [ "$(json_string_field "$lock" manifest_path)" = "${CODESCAN_RELATIVE}/manifest.toml" ] &&
    [ "$(json_string_field "$lock" manifest_digest)" = "$(sha256 "$manifest")" ] &&
    [ "$(json_string_field "$lock" executable_path)" = "${CODESCAN_RELATIVE}/target/release/omegon-codescan" ] &&
    [ "$(json_string_field "$lock" executable_digest)" = "$(sha256 "$executable")" ] &&
    [ "$(json_string_field "$lock" target)" = "$PLATFORM" ] &&
    [ "$(json_number_field "$lock" protocol_minimum)" = "1" ] &&
    [ "$(json_number_field "$lock" protocol_maximum)" = "1" ] &&
    [ "$(json_number_field "$lock" protocol_version)" = "1" ] &&
    [ "$(json_string_field "$lock" fallback)" = "typed_unavailable" ] &&
    [ "$(json_string_field "$lock" issuer)" = "https://token.actions.githubusercontent.com" ] &&
    [ "$(json_string_field "$lock" verification)" = "required" ] ||
    die "release-coupled codescan component lock is invalid"
  case "$(json_string_field "$lock" workflow_identity)" in
    https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v*) ;;
    *) die "release-coupled codescan signing identity is invalid" ;;
  esac
}

validate_full_generation() {
  generation="$1"
  expected_version="$2"
  [ -d "$generation" ] && [ ! -L "$generation" ] ||
    die "release generation is not an immutable directory: ${generation}"
  for executable in "$BINARY" "$MAINTAIN_BINARY" "${CODESCAN_RELATIVE}/target/release/omegon-codescan"; do
    [ -x "${generation}/${executable}" ] ||
      die "release generation executable is missing: ${executable}"
  done
  for member in \
    "${BINARY}.composition-lock.json" \
    "${MAINTAIN_BINARY}.composition-lock.json" \
    "${PACK_RELATIVE}/content-pack.toml" \
    "${CODESCAN_RELATIVE}/manifest.toml" \
    "$COMPONENT_LOCK_RELATIVE" \
    "install-receipt.json"; do
    [ -f "${generation}/${member}" ] ||
      die "release generation member is missing: ${member}"
  done
  [ "$(json_string_field "${generation}/install-receipt.json" version)" = "$expected_version" ] &&
    [ "$(json_string_field "${generation}/install-receipt.json" layout)" = "versioned-current-v1" ] ||
    die "release generation receipt is invalid: ${generation}"
  validate_resident_lock "$generation" "$BINARY" "omegon"
  validate_resident_lock "$generation" "$MAINTAIN_BINARY" "omegon-maintain"
  validate_product_component "$generation"
}

# ── Platform detection ────────────────────────────────────────

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) OS_NAME="darwin" ;;
  linux)  OS_NAME="linux" ;;
  *)
    die "unsupported OS: $OS (omegon supports macOS and Linux; Windows users: use WSL)"
    ;;
esac

case "$ARCH" in
  arm64|aarch64) ARCH_NAME="aarch64" ;;
  x86_64|amd64)  ARCH_NAME="x86_64" ;;
  *)
    die "unsupported architecture: $ARCH"
    ;;
esac

# Build Rust target triple to match release asset names
# Assets are: omegon-{VERSION}-{TARGET}.tar.gz
# e.g. omegon-0.15.2-aarch64-apple-darwin.tar.gz
case "$OS_NAME" in
  darwin) TARGET="${ARCH_NAME}-apple-darwin" ;;
  linux)
    # Prefer musl (static, works on NixOS/Alpine/containers) over gnu.
    # Fall back to gnu if musl asset doesn't exist in the release.
    if [ "$ARCH_NAME" = "x86_64" ]; then
      TARGET="${ARCH_NAME}-unknown-linux-musl"
      TARGET_FALLBACK="${ARCH_NAME}-unknown-linux-gnu"
    else
      TARGET="${ARCH_NAME}-unknown-linux-gnu"
    fi
    ;;
esac

PLATFORM="${TARGET}"
CHECKSUMS="checksums.sha256"

# ── Banner ────────────────────────────────────────────────────

echo ""
printf "${BOLD}${CYAN}  Ω  Omegon Installer${RESET}\n"
printf "${DIM}  Native AI agent harness — single binary, zero dependencies${RESET}\n"
echo ""

# ── Version resolution ────────────────────────────────────────

if [ -z "$VERSION" ] && [ -n "$LOCAL_ARCHIVE" ]; then
  die "VERSION is required with OMEGON_INSTALL_ARCHIVE"
fi
if [ -z "$VERSION" ]; then
  case "$CHANNEL" in
    stable)
      step "Resolving latest stable release..."
      RELEASE_JSON=$(curl -fsSL "${GITHUB_API}/releases/latest" 2>/dev/null) || \
        die "could not reach GitHub API. Check your network connection."
      VERSION=$(printf '%s' "$RELEASE_JSON" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
      ;;
    rc)
      # RC channel deprecated — redirect to stable.
      warn "RC channel is deprecated. Use 'stable' or 'nightly'. Installing latest stable."
      CHANNEL="stable"
      step "Resolving latest stable release..."
      RELEASE_JSON=$(curl -fsSL "${GITHUB_API}/releases/latest" 2>/dev/null) || \
        die "could not reach GitHub API. Check your network connection."
      VERSION=$(printf '%s' "$RELEASE_JSON" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
      ;;
    nightly)
      step "Resolving latest ${CHANNEL} release..."
      RELEASE_JSON=$(curl -fsSL "${GITHUB_API}/releases" 2>/dev/null) || \
        die "could not reach GitHub API. Check your network connection."
      # Pure shell — no python dependency. Extracts the first prerelease
      # tag_name containing the nightly marker.
      MARKER="-nightly."
      VERSION=$(printf '%s' "$RELEASE_JSON" | grep -o '"tag_name": *"[^"]*'"${MARKER}"'[^"]*"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
      ;;
    *)
      die "unsupported CHANNEL: $CHANNEL (expected stable, rc, or nightly)"
      ;;
  esac

  if [ -z "$VERSION" ]; then
    die "could not determine latest ${CHANNEL} release. Check: https://github.com/${REPO}/releases"
  fi
fi
case "$VERSION" in
  ""|.|..|*[!A-Za-z0-9._+-]*) die "VERSION contains characters unsafe for a generation name" ;;
esac

# Strip 'v' prefix from version for asset names (tags are v0.15.2, assets are omegon-0.15.2-...)
VERSION_NUM=$(echo "$VERSION" | sed 's/^v//')

# Construct the archive name to match release assets
ARCHIVE="${BINARY}-${VERSION_NUM}-${PLATFORM}.tar.gz"
if [ -n "$LOCAL_ARCHIVE" ]; then
  ARCHIVE=$(basename "$LOCAL_ARCHIVE")
  case "$ARCHIVE" in
    *-aarch64-apple-darwin.tar.gz) PLATFORM="aarch64-apple-darwin" ;;
    *-x86_64-apple-darwin.tar.gz) PLATFORM="x86_64-apple-darwin" ;;
    *-aarch64-unknown-linux-gnu.tar.gz) PLATFORM="aarch64-unknown-linux-gnu" ;;
    *-x86_64-unknown-linux-gnu.tar.gz) PLATFORM="x86_64-unknown-linux-gnu" ;;
    *-x86_64-unknown-linux-musl.tar.gz) PLATFORM="x86_64-unknown-linux-musl" ;;
    *) die "local release archive filename does not identify a supported target" ;;
  esac
  [ "$ARCHIVE" = "${BINARY}-${VERSION_NUM}-${PLATFORM}.tar.gz" ] ||
    die "local release archive filename does not match VERSION and target"
fi

# ── Installation plan ─────────────────────────────────────────

NEEDS_SUDO=false
if [ -d "$INSTALL_DIR" ] && [ ! -w "$INSTALL_DIR" ]; then
  NEEDS_SUDO=true
elif [ ! -d "$INSTALL_DIR" ] && ! mkdir -p "$INSTALL_DIR" 2>/dev/null; then
  NEEDS_SUDO=true
  rmdir "$INSTALL_DIR" 2>/dev/null || true
fi

EXISTING=""
if [ -x "${INSTALL_DIR}/${BINARY}" ]; then
  EXISTING=$("${INSTALL_DIR}/${BINARY}" --version 2>/dev/null | head -1 || true)
  [ -z "$EXISTING" ] && EXISTING="unknown"
fi

printf "  ${BOLD}Installation Plan${RESET}\n"
printf "  ${DIM}────────────────────────────────────────${RESET}\n"
printf "  ${CYAN}Version:${RESET}     %s\n" "${VERSION}"
printf "  ${CYAN}Channel:${RESET}     %s\n" "${CHANNEL}"
printf "  ${CYAN}Platform:${RESET}    %s\n" "${PLATFORM}"
printf "  ${CYAN}Install to:${RESET}  ~/.omegon/versions/%s/omegon\n" "${VERSION}"
printf "  ${CYAN}Symlink at:${RESET}  %s\n" "${INSTALL_DIR}/${BINARY}"
if [ -n "$EXISTING" ]; then
  printf "  ${YELLOW}Replaces:${RESET}    %s\n" "${EXISTING}"
fi
if [ "$NEEDS_SUDO" = true ]; then
  printf "  ${YELLOW}Requires:${RESET}    sudo (%s is not writable)\n" "${INSTALL_DIR}"
fi
printf "  ${DIM}Source: github.com/%s${RESET}\n" "${REPO}"
printf "  ${DIM}Authenticity: required external package-manifest and Sigstore-bundle verification${RESET}\n"
printf "  ${DIM}Integrity aid: SHA-256 checksum verification${RESET}\n"
echo ""

# ── Confirmation ──────────────────────────────────────────────

if [ "$NO_CONFIRM" = false ] && [ -t 0 ]; then
  printf "  Proceed with installation? ${DIM}[Y/n]${RESET} "
  read -r REPLY < /dev/tty || REPLY="y"
  case "$REPLY" in
    [nN]*) echo "  Cancelled."; exit 0 ;;
  esac
  echo ""
fi

# ── Download ──────────────────────────────────────────────────

BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
ARCHIVE_URL="${BASE_URL}/${ARCHIVE}"
CHECKSUMS_URL="${BASE_URL}/${CHECKSUMS}"

TMP=$(mktemp -d) || die "could not create temporary directory"

if [ -n "$LOCAL_ARCHIVE" ]; then
  step "Loading local ${ARCHIVE}..."
  cp "$LOCAL_ARCHIVE" "${TMP}/${ARCHIVE}" || die "could not stage local release archive"
else
  step "Downloading ${ARCHIVE}..."
  HTTP_CODE=$(curl -fSL -w '%{http_code}' -o "${TMP}/${ARCHIVE}" "$ARCHIVE_URL" 2>/dev/null) || true
fi

if [ -z "$LOCAL_ARCHIVE" ] && { [ ! -f "${TMP}/${ARCHIVE}" ] || [ "$HTTP_CODE" = "404" ]; }; then
  # Fallback: if musl target not available, try gnu
  if [ -n "${TARGET_FALLBACK:-}" ]; then
    step "Static (musl) build not available, falling back to gnu..."
    PLATFORM="${TARGET_FALLBACK}"
    VERSION_NUM=$(echo "$VERSION" | sed 's/^v//')
    ARCHIVE="${BINARY}-${VERSION_NUM}-${TARGET_FALLBACK}.tar.gz"
    ARCHIVE_URL="${BASE_URL}/${ARCHIVE}"
    rm -f "${TMP}/${ARCHIVE}" 2>/dev/null
    HTTP_CODE=$(curl -fSL -w '%{http_code}' -o "${TMP}/${ARCHIVE}" "$ARCHIVE_URL" 2>/dev/null) || true
  fi
  if [ ! -f "${TMP}/${ARCHIVE}" ] || [ "$HTTP_CODE" = "404" ]; then
    die "release artifact not found: ${ARCHIVE_URL}

  Available targets: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu, x86_64-unknown-linux-musl, aarch64-unknown-linux-gnu
  Check releases: https://github.com/${REPO}/releases/tag/${VERSION}"
  fi
fi

MANIFEST="${ARCHIVE}.manifest.json"
BUNDLE="${ARCHIVE}.manifest.sigstore.json"
MANIFEST_URL="${BASE_URL}/${MANIFEST}"
BUNDLE_URL="${BASE_URL}/${BUNDLE}"

ARCHIVE_SIZE=$(wc -c < "${TMP}/${ARCHIVE}" | tr -d ' ')
if [ "$ARCHIVE_SIZE" -lt 1000 ]; then
  die "downloaded archive is too small (${ARCHIVE_SIZE} bytes) — likely a failed download"
fi

ok "Downloaded $(dimtext "${ARCHIVE_SIZE} bytes")"

# ── Checksum verification ─────────────────────────────────────

step "Verifying checksum..."

if [ -n "$LOCAL_CHECKSUMS" ]; then
  cp "$LOCAL_CHECKSUMS" "${TMP}/${CHECKSUMS}" || die "could not stage local checksums"
  CHECKSUM_AVAILABLE=true
elif curl -fsSL -o "${TMP}/${CHECKSUMS}" "$CHECKSUMS_URL" 2>/dev/null; then
  CHECKSUM_AVAILABLE=true
else
  CHECKSUM_AVAILABLE=false
fi
if [ "$CHECKSUM_AVAILABLE" = true ]; then
  EXPECTED=$(grep "${ARCHIVE}" "${TMP}/${CHECKSUMS}" | cut -d' ' -f1)

  if [ -z "$EXPECTED" ]; then
    die "checksum for ${ARCHIVE} not found in ${CHECKSUMS}"
  fi

  ACTUAL=$(sha256 "${TMP}/${ARCHIVE}")

  if [ "$EXPECTED" != "$ACTUAL" ]; then
    die "checksum mismatch!
    Expected: ${EXPECTED}
    Actual:   ${ACTUAL}

    The download may be corrupted or tampered with.
    Try again, or download manually from:
      https://github.com/${REPO}/releases/tag/${VERSION}"
  fi

  SHORT_HASH=$(printf '%.12s' "$ACTUAL")
  ok "Checksum verified $(dimtext "${SHORT_HASH}…")"
else
  warn "Checksum file not available for this release — skipping verification"
fi

# ── External bootstrap authentication ─────────────────────────

if [ -n "$LOCAL_ARCHIVE" ]; then
  cp "$LOCAL_MANIFEST" "${TMP}/${MANIFEST}" || die "could not stage local package manifest"
  cp "$LOCAL_BUNDLE" "${TMP}/${BUNDLE}" || die "could not stage local Sigstore bundle"
else
  step "Downloading signed package evidence..."
  curl -fsSL -o "${TMP}/${MANIFEST}" "$MANIFEST_URL" 2>/dev/null ||
    die "package manifest not available for ${ARCHIVE}"
  curl -fsSL -o "${TMP}/${BUNDLE}" "$BUNDLE_URL" 2>/dev/null ||
    die "Sigstore bundle not available for ${ARCHIVE}"
fi

step "Authenticating release with the external bootstrap verifier..."
if ! "$BOOTSTRAP_VERIFIER" --json release verify \
    --archive "${TMP}/${ARCHIVE}" \
    --manifest "${TMP}/${MANIFEST}" \
    --bundle "${TMP}/${BUNDLE}" \
    >"${TMP}/bootstrap-verification.json" 2>"${TMP}/bootstrap-verification.err"; then
  die "external bootstrap verifier refused the release; checksum success cannot grant authenticity"
fi
verification_result_succeeded "${TMP}/bootstrap-verification.json" ||
  die "external bootstrap verifier returned malformed or incomplete success evidence"
ok "Release authenticity and signed composition verified"

# ── Extract ───────────────────────────────────────────────────

step "Extracting authenticated archive..."

EXTRACTED_ROOT="${TMP}/extracted"
mkdir "$EXTRACTED_ROOT" || die "could not create authenticated extraction root"
tar xzf "${TMP}/${ARCHIVE}" -C "$EXTRACTED_ROOT" 2>/dev/null || \
  die "failed to extract ${ARCHIVE} — the download may be corrupted"

for REQUIRED_BINARY in "$BINARY" "$MAINTAIN_BINARY"; do
  if [ ! -f "${EXTRACTED_ROOT}/${REQUIRED_BINARY}" ]; then
    die "binary '${REQUIRED_BINARY}' not found in archive — release companion pair is incomplete"
  fi
done
for REQUIRED_LOCK in "${BINARY}.composition-lock.json" "${MAINTAIN_BINARY}.composition-lock.json"; do
  [ -f "${EXTRACTED_ROOT}/${REQUIRED_LOCK}" ] || die "resident lock '${REQUIRED_LOCK}' not found in archive"
done
PACK_RELATIVE="share/omegon/content-packs/omegon-shipped"
if [ ! -f "${EXTRACTED_ROOT}/${PACK_RELATIVE}/content-pack.toml" ]; then
  die "shipped content pack not found in archive — optional content installation is incomplete"
fi
CODESCAN_RELATIVE="share/omegon/extensions/omegon-codescan"
COMPONENT_LOCK_RELATIVE="share/omegon/components/core-codescan.lock.json"
if [ ! -f "${EXTRACTED_ROOT}/${CODESCAN_RELATIVE}/manifest.toml" ] || \
   [ ! -f "${EXTRACTED_ROOT}/${CODESCAN_RELATIVE}/target/release/omegon-codescan" ] || \
   [ ! -f "${EXTRACTED_ROOT}/${COMPONENT_LOCK_RELATIVE}" ]; then
  die "release-coupled codescan extension not found in archive"
fi

# ── Validate binary ───────────────────────────────────────────

for REQUIRED_BINARY in "$BINARY" "$MAINTAIN_BINARY" "${CODESCAN_RELATIVE}/target/release/omegon-codescan"; do
  FIRST_BYTES=$(head -c 4 "${EXTRACTED_ROOT}/${REQUIRED_BINARY}" | xxd -p 2>/dev/null || od -A n -t x1 -N 4 "${EXTRACTED_ROOT}/${REQUIRED_BINARY}" | tr -d ' ')
  case "$OS_NAME" in
    darwin)
      case "$FIRST_BYTES" in
        feedface*|feedfacf*|cafebabe*|cffaedfe*|cffa*) ;;
        *) die "downloaded ${REQUIRED_BINARY} is not a valid macOS binary (magic: ${FIRST_BYTES})" ;;
      esac
      ;;
    linux)
      case "$FIRST_BYTES" in
        7f454c46*) ;;
        *) die "downloaded ${REQUIRED_BINARY} is not a valid Linux binary (magic: ${FIRST_BYTES})" ;;
      esac
      ;;
  esac
done

step "Revalidating the extracted member tree with authenticated omegon-maintain..."
if ! "${EXTRACTED_ROOT}/${MAINTAIN_BINARY}" --json release verify \
    --archive "${TMP}/${ARCHIVE}" \
    --manifest "${TMP}/${MANIFEST}" \
    --bundle "${TMP}/${BUNDLE}" \
    --extracted-root "$EXTRACTED_ROOT" \
    >"${TMP}/extracted-verification.json" 2>"${TMP}/extracted-verification.err"; then
  die "authenticated omegon-maintain refused the exact extracted archive member tree"
fi
verification_result_succeeded "${TMP}/extracted-verification.json" ||
  die "authenticated omegon-maintain returned malformed or incomplete extracted-tree evidence"

OMEGON_STAGED_VERSION=$("${EXTRACTED_ROOT}/${BINARY}" --version 2>/dev/null | head -1 | awk '{print $2}') || die "staged omegon failed to launch"
MAINTAIN_STAGED_VERSION=$("${EXTRACTED_ROOT}/${MAINTAIN_BINARY}" --version 2>/dev/null | head -1 | awk '{print $2}') || die "staged omegon-maintain failed to launch"
if [ -z "$OMEGON_STAGED_VERSION" ] || [ "$OMEGON_STAGED_VERSION" != "$MAINTAIN_STAGED_VERSION" ]; then
  die "release companion version mismatch: omegon=${OMEGON_STAGED_VERSION:-missing}, omegon-maintain=${MAINTAIN_STAGED_VERSION:-missing}"
fi
ok "Release companion pair validated"

# ── NixOS compatibility check ────────────────────────────────

if [ -f /etc/NIXOS ] || [ -d /nix/store ]; then
  warn "NixOS detected — prebuilt binaries require nix-ld or an FHS wrapper"
  echo ""
  printf "  ${DIM}Option 1: Enable nix-ld (recommended, system-wide):${RESET}\n"
  printf "  ${DIM}  Add to your NixOS configuration:${RESET}\n"
  printf "  ${DIM}    programs.nix-ld.enable = true;${RESET}\n"
  printf "  ${DIM}    programs.nix-ld.libraries = with pkgs; [ stdenv.cc.cc ];${RESET}\n"
  echo ""
  printf "  ${DIM}Option 2: Run with steam-run (per-session):${RESET}\n"
  printf "  ${DIM}    nix-shell -p steam-run --run omegon${RESET}\n"
  echo ""
  printf "  ${DIM}Option 3: Build from source:${RESET}\n"
  printf "  ${DIM}    cargo install --git https://github.com/${REPO}${RESET}\n"
  echo ""
fi

# ── Install ───────────────────────────────────────────────────

VERSIONS_DIR="${HOME}/.omegon/versions"
VERSION_DIR="${VERSIONS_DIR}/${VERSION}"
CURRENT_LINK="${HOME}/.omegon/current"
INSTALL_TARGET="${INSTALL_DIR}/${BINARY}"
MAINTAIN_INSTALL_TARGET="${INSTALL_DIR}/${MAINTAIN_BINARY}"
OM_TARGET="${INSTALL_DIR}/om"
RECEIPT_PATH="${RECEIPT_DIR}/install-receipt.json"

step "Installing ${VERSION}..."

mkdir -p "$VERSIONS_DIR" "$RECEIPT_DIR" || die "could not create release directories"
STAGING_DIR="${VERSIONS_DIR}/.${VERSION}.staging.$$"
rm -rf "$STAGING_DIR"
mkdir "$STAGING_DIR" || die "could not create release staging directory"
mv "${EXTRACTED_ROOT}/${BINARY}" "${STAGING_DIR}/${BINARY}"
mv "${EXTRACTED_ROOT}/${MAINTAIN_BINARY}" "${STAGING_DIR}/${MAINTAIN_BINARY}"
mv "${EXTRACTED_ROOT}/${BINARY}.composition-lock.json" "${STAGING_DIR}/${BINARY}.composition-lock.json"
mv "${EXTRACTED_ROOT}/${MAINTAIN_BINARY}.composition-lock.json" "${STAGING_DIR}/${MAINTAIN_BINARY}.composition-lock.json"
mkdir -p "${STAGING_DIR}/share/omegon/content-packs"
mv "${EXTRACTED_ROOT}/${PACK_RELATIVE}" "${STAGING_DIR}/${PACK_RELATIVE}"
mkdir -p "${STAGING_DIR}/share/omegon/extensions"
mv "${EXTRACTED_ROOT}/${CODESCAN_RELATIVE}" "${STAGING_DIR}/${CODESCAN_RELATIVE}"
mkdir -p "${STAGING_DIR}/share/omegon/components"
mv "${EXTRACTED_ROOT}/${COMPONENT_LOCK_RELATIVE}" "${STAGING_DIR}/${COMPONENT_LOCK_RELATIVE}"
chmod +x "${STAGING_DIR}/${BINARY}" "${STAGING_DIR}/${MAINTAIN_BINARY}" || \
  die "could not make release pair executable"
chmod +x "${STAGING_DIR}/${CODESCAN_RELATIVE}/target/release/omegon-codescan" || \
  die "could not make codescan extension executable"

cat > "${STAGING_DIR}/install-receipt.json" <<EOF
{
  "version": "${VERSION}",
  "platform": "${PLATFORM}",
  "install_dir": "${INSTALL_DIR}",
  "binary": "${INSTALL_TARGET}",
  "maintenance_binary": "${MAINTAIN_INSTALL_TARGET}",
  "version_dir": "${VERSION_DIR}",
  "versioned_binary": "${VERSION_DIR}/${BINARY}",
  "versioned_maintenance_binary": "${VERSION_DIR}/${MAINTAIN_BINARY}",
  "activation": "${CURRENT_LINK}",
  "installed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "source": "https://github.com/${REPO}/releases/tag/${VERSION}",
  "installer": "https://omegon.styrene.io/install.sh",
  "layout": "versioned-current-v1"
}
EOF

validate_full_generation "$STAGING_DIR" "$VERSION"

# Flush the complete candidate before publishing its immutable directory.
sync
if [ -e "$VERSION_DIR" ] || [ -L "$VERSION_DIR" ]; then
  die "release generation already exists; refusing to substitute it for the authenticated candidate: ${VERSION_DIR}"
else
  mv "$STAGING_DIR" "$VERSION_DIR" || die "could not publish release generation"
  STAGING_DIR=""
  PUBLISHED_CANDIDATE="$VERSION_DIR"
  sync
fi

# Create install directory if needed
if [ ! -d "$INSTALL_DIR" ]; then
  if [ "$NEEDS_SUDO" = true ]; then
    sudo mkdir -p "$INSTALL_DIR" || die "could not create ${INSTALL_DIR}"
  else
    mkdir -p "$INSTALL_DIR"
  fi
fi

atomic_link() {
  link_target="$1"
  link_path="$2"
  temp_link="${link_path}.tmp.$$"
  if [ -d "$link_path" ] && [ ! -L "$link_path" ]; then
    die "refusing to replace directory at ${link_path}"
  fi
  if [ "$NEEDS_SUDO" = true ] && [ "${link_path#${INSTALL_DIR}/}" != "$link_path" ]; then
    sudo rm -f "$temp_link"
    sudo ln -s "$link_target" "$temp_link" || die "could not stage ${link_path}"
    if ! sudo mv -f -T "$temp_link" "$link_path" 2>/dev/null && \
       ! sudo mv -f -h "$temp_link" "$link_path" 2>/dev/null; then
      sudo rm -f "$temp_link"
      die "could not atomically update ${link_path}"
    fi
  else
    rm -f "$temp_link"
    ln -s "$link_target" "$temp_link" || die "could not stage ${link_path}"
    if ! mv -f -T "$temp_link" "$link_path" 2>/dev/null && \
       ! mv -f -h "$temp_link" "$link_path" 2>/dev/null; then
      rm -f "$temp_link"
      die "could not atomically update ${link_path}"
    fi
  fi
}

# Migrate a complete old direct-link generation through `current` before the
# version-changing activation. During migration every changed link still names
# the old pair.
if [ ! -L "$CURRENT_LINK" ] && [ -L "$INSTALL_TARGET" ]; then
  OLD_BINARY=$(readlink "$INSTALL_TARGET")
  OLD_DIR=$(dirname "$OLD_BINARY")
  if [ -x "${OLD_DIR}/${BINARY}" ] && [ -x "${OLD_DIR}/${MAINTAIN_BINARY}" ]; then
    if [ ! -f "${OLD_DIR}/install-receipt.json" ] && [ -f "$RECEIPT_PATH" ]; then
      cp "$RECEIPT_PATH" "${OLD_DIR}/install-receipt.json" || \
        die "could not migrate prior generation receipt"
    fi
    [ -f "${OLD_DIR}/install-receipt.json" ] || \
      die "prior release generation has no receipt"
    OLD_VERSION=$(json_string_field "${OLD_DIR}/install-receipt.json" version)
    validate_full_generation "$OLD_DIR" "$OLD_VERSION"
    atomic_link "$OLD_DIR" "$CURRENT_LINK"
    atomic_link "${CURRENT_LINK}/${BINARY}" "$INSTALL_TARGET"
    atomic_link "${CURRENT_LINK}/${BINARY}" "$OM_TARGET"
    atomic_link "${CURRENT_LINK}/${MAINTAIN_BINARY}" "$MAINTAIN_INSTALL_TARGET"
    atomic_link "${CURRENT_LINK}/install-receipt.json" "$RECEIPT_PATH"
  fi
fi

# Prepare stable launch links while they still resolve through the old selector.
atomic_link "${CURRENT_LINK}/${BINARY}" "$INSTALL_TARGET"
atomic_link "${CURRENT_LINK}/${BINARY}" "$OM_TARGET"
atomic_link "${CURRENT_LINK}/${MAINTAIN_BINARY}" "$MAINTAIN_INSTALL_TARGET"
atomic_link "${CURRENT_LINK}/install-receipt.json" "$RECEIPT_PATH"

# This final rename is the only operation that changes the selected generation.
atomic_link "$VERSION_DIR" "$CURRENT_LINK"
INSTALL_SUCCEEDED=true
PUBLISHED_CANDIDATE=""

# ── Verify installation ──────────────────────────────────────

INSTALLED_VERSION="omegon ${OMEGON_STAGED_VERSION}"
if command -v "$BINARY" >/dev/null 2>&1; then
  ok "Installed to ${BOLD}${VERSION_DIR}/${BINARY}${RESET}"
  ok "Symlinked from ${BOLD}${INSTALL_DIR}/${BINARY}${RESET}"
elif [ -x "${INSTALL_DIR}/${BINARY}" ]; then
  ok "Installed to ${BOLD}${VERSION_DIR}/${BINARY}${RESET}"
  ok "Symlinked from ${BOLD}${INSTALL_DIR}/${BINARY}${RESET}"
  # Auto-wire PATH if needed
  if ! echo "$PATH" | tr ':' '\n' | grep -qx "${INSTALL_DIR}"; then
    SHELL_RC=""
    CURRENT_SHELL="$(basename "${SHELL:-sh}")"
    if [ "$CURRENT_SHELL" = "zsh" ] && [ -f "$HOME/.zshrc" ]; then
      SHELL_RC="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
      SHELL_RC="$HOME/.bashrc"
    elif [ -f "$HOME/.profile" ]; then
      SHELL_RC="$HOME/.profile"
    fi
    if [ -n "$SHELL_RC" ] && ! grep -q "${INSTALL_DIR}" "$SHELL_RC" 2>/dev/null; then
      printf '\n# Omegon (added by installer)\nexport PATH="%s:$PATH"\n' "${INSTALL_DIR}" >> "$SHELL_RC"
      ok "Added ${INSTALL_DIR} to PATH in ${SHELL_RC}"
      printf "${DIM}    Run: source ${SHELL_RC}   (or open a new terminal)${RESET}\n"
    else
      warn "${INSTALL_DIR} is not in your PATH"
      printf "${DIM}    Add it: export PATH=\"${INSTALL_DIR}:\$PATH\"${RESET}\n"
    fi
  fi
else
  die "installation failed — ${INSTALL_DIR}/${BINARY} is not executable"
fi
if [ ! -x "${INSTALL_DIR}/${MAINTAIN_BINARY}" ]; then
  die "installation failed — ${INSTALL_DIR}/${MAINTAIN_BINARY} is not executable"
fi
ok "Companion available at ${BOLD}${INSTALL_DIR}/${MAINTAIN_BINARY}${RESET}"

# ── Summary ───────────────────────────────────────────────────

echo ""
printf "${BOLD}${GREEN}  ✓ Omegon %s installed successfully${RESET}\n" "${VERSION}"
if [ -n "$INSTALLED_VERSION" ]; then
  printf "${DIM}    %s${RESET}\n" "${INSTALLED_VERSION}"
fi
printf "${DIM}    Receipt: %s/install-receipt.json${RESET}\n" "${RECEIPT_DIR}"
echo ""
printf "  ${BOLD}Quick start${RESET}\n"
printf "  ${DIM}────────────────────────────────────────${RESET}\n"
printf "  ${CYAN}With API key:${RESET}\n"
printf "    ${DIM}export ANTHROPIC_API_KEY=\"sk-ant-...\"${RESET}\n"
printf "    omegon\n"
echo ""
printf "  ${CYAN}With Claude Pro/Max subscription:${RESET}\n"
printf "    omegon login\n"
echo ""
printf "  ${CYAN}One-shot:${RESET}\n"
printf "    omegon --prompt \"hello world\"\n"
echo ""
printf "  ${DIM}Uninstall:${RESET}\n"
printf "    ${DIM}rm %s/%s${RESET}\n" "${INSTALL_DIR}" "${BINARY}"
printf "    ${DIM}rm -rf ~/.omegon/versions${RESET}\n"
printf "    ${DIM}rm -rf ~/.config/omegon${RESET}\n"
echo ""
# Rebuilt 2026-04-27
