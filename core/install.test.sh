#!/bin/bash
# End-to-end trust, cleanup, and activation contracts for install.sh.

set -eu

TEST_DIR="$(mktemp -d)"
TEST_HOME="${TEST_DIR}/home"
INSTALL_DIR="${TEST_DIR}/bin"
FIXTURES="${TEST_DIR}/fixtures"
FAKE_BIN="${TEST_DIR}/fake-bin"
INSTALLER_TMP="${TEST_DIR}/installer-tmp"
EVENTS="${TEST_DIR}/events"
AUTH_MARKER="${TEST_DIR}/authenticated"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REAL_TAR="$(command -v tar)"

cleanup() { rm -rf "$TEST_DIR"; }
trap cleanup EXIT

case "$(uname -s)" in
    Darwin)
        case "$(uname -m)" in
            arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
            *) TARGET="x86_64-apple-darwin" ;;
        esac
        MAGIC="feedfacf"
        ;;
    Linux)
        case "$(uname -m)" in
            arm64|aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
            *) TARGET="x86_64-unknown-linux-musl" ;;
        esac
        MAGIC="7f454c46"
        ;;
esac

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

file_mode() {
    if stat -f '%Lp' "$1" >/dev/null 2>&1; then
        stat -f '%Lp' "$1"
    else
        stat -c '%a' "$1"
    fi
}

make_archive() {
    version="$1"
    component_state="$2"
    root="${FIXTURES}/${version}/root"
    archive="${FIXTURES}/${version}/omegon-${version}-${TARGET}.tar.gz"
    manifest="${archive}.manifest.json"
    bundle="${archive}.manifest.sigstore.json"
    mkdir -p "${root}/share/omegon/content-packs/omegon-shipped"
    mkdir -p "${root}/share/omegon/extensions/omegon-codescan/target/release"
    mkdir -p "${root}/share/omegon/components"
    cat > "${root}/omegon" <<'EOF'
#!/bin/sh
if [ -n "${EVENTS:-}" ]; then printf 'candidate-omegon\n' >> "$EVENTS"; fi
printf 'omegon %s\n' "$FIXTURE_VERSION"
EOF
    cat > "${root}/omegon-maintain" <<'EOF'
#!/bin/sh
if [ -n "${EVENTS:-}" ]; then printf 'candidate-maintain\n' >> "$EVENTS"; fi
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}
file_mode() {
    if stat -f '%Lp' "$1" >/dev/null 2>&1; then
        stat -f '%Lp' "$1"
    else
        stat -c '%a' "$1"
    fi
}
if [ "${1:-}" = "--version" ]; then
    printf 'omegon-maintain %s\n' "$FIXTURE_VERSION"
    exit 0
fi
manifest=""
extracted_root=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --manifest) manifest="$2"; shift 2 ;;
        --extracted-root) extracted_root="$2"; shift 2 ;;
        *) shift ;;
    esac
done
[ -n "$manifest" ] && [ -n "$extracted_root" ] && [ -f "$AUTH_MARKER" ] || exit 1
[ "${CANDIDATE_VERIFY_REFUSE:-0}" != 1 ] || exit 1
expected=0
while IFS=' ' read -r digest mode size relative; do
    [ -n "$relative" ] || exit 1
    path="$extracted_root/$relative"
    [ -f "$path" ] && [ ! -L "$path" ] || exit 1
    [ "$(sha256 "$path")" = "$digest" ] || exit 1
    [ "$(file_mode "$path")" = "$mode" ] || exit 1
    [ "$(wc -c < "$path" | tr -d ' ')" = "$size" ] || exit 1
    expected=$((expected + 1))
done < "$manifest"
actual=$(find "$extracted_root" -type f | wc -l | tr -d ' ')
special=$(find "$extracted_root" ! -type f ! -type d | wc -l | tr -d ' ')
[ "$actual" = "$expected" ] && [ "$special" = 0 ] || exit 1
if [ "${CANDIDATE_RESULT_MALFORMED:-0}" = 1 ]; then
    printf '{"status":"success"}\n'
else
    printf '{"status":"success","diagnostics":[{"code":"release_verified"}]}\n'
fi
EOF
    cat > "${root}/share/omegon/extensions/omegon-codescan/target/release/omegon-codescan" <<'EOF'
#!/bin/sh
printf 'codescan %s\n' "$FIXTURE_VERSION"
EOF
    chmod 0755 "${root}/omegon" "${root}/omegon-maintain" \
        "${root}/share/omegon/extensions/omegon-codescan/target/release/omegon-codescan"
    printf 'id = "omegon-shipped"\n# %01200d\n' 0 \
        > "${root}/share/omegon/content-packs/omegon-shipped/content-pack.toml"
    printf 'name = "omegon-codescan"\n' \
        > "${root}/share/omegon/extensions/omegon-codescan/manifest.toml"

    for executable in omegon omegon-maintain; do
        digest="$(sha256 "${root}/${executable}")"
        printf '{"schema_version":1,"executable_identity":"%s","executable_digest":"%s","target":"%s","protocol_minimum":1,"protocol_maximum":1,"contributions":[],"signing_identity":{"issuer":"https://token.actions.githubusercontent.com","workflow_identity":"https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v%s","verification":"required"}}\n' \
            "$executable" "$digest" "$TARGET" "$version" \
            > "${root}/${executable}.composition-lock.json"
    done

    if [ "$component_state" != missing ]; then
        component_manifest="${root}/share/omegon/extensions/omegon-codescan/manifest.toml"
        component_executable="${root}/share/omegon/extensions/omegon-codescan/target/release/omegon-codescan"
        if [ "$component_state" = corrupt ]; then
            printf '{}\n' > "${root}/share/omegon/components/core-codescan.lock.json"
        else
            printf '{"schema_version":1,"component_id":"core:codescan","wire_manifest_id":"omegon-codescan","manifest_path":"share/omegon/extensions/omegon-codescan/manifest.toml","manifest_digest":"%s","executable_path":"share/omegon/extensions/omegon-codescan/target/release/omegon-codescan","executable_digest":"%s","target":"%s","protocol_minimum":1,"protocol_maximum":1,"protocol_version":1,"fallback":"typed_unavailable","signing_identity":{"issuer":"https://token.actions.githubusercontent.com","workflow_identity":"https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v%s","verification":"required"}}\n' \
                "$(sha256 "$component_manifest")" "$(sha256 "$component_executable")" "$TARGET" "$version" \
                > "${root}/share/omegon/components/core-codescan.lock.json"
        fi
    fi

    : > "$manifest"
    find "$root" -type f | LC_ALL=C sort | while IFS= read -r path; do
        relative="${path#${root}/}"
        printf '%s %s %s %s\n' "$(sha256 "$path")" "$(file_mode "$path")" \
            "$(wc -c < "$path" | tr -d ' ')" "$relative" >> "$manifest"
    done
    printf 'trusted fixture bundle\n' > "$bundle"
    tar czf "$archive" -C "$root" .
    printf '%s  %s\n' "$(sha256 "$archive")" "$(basename "$archive")" \
        > "${FIXTURES}/${version}/checksums.sha256"
}

mkdir -p "$TEST_HOME" "$INSTALL_DIR" "$FIXTURES" "$FAKE_BIN" "$INSTALLER_TMP"
: > "$EVENTS"

cat > "${FAKE_BIN}/xxd" <<EOF
#!/bin/sh
cat >/dev/null
printf '%s\n' '$MAGIC'
EOF
chmod +x "${FAKE_BIN}/xxd"

cat > "${FAKE_BIN}/tar" <<'EOF'
#!/bin/sh
printf 'extract\n' >> "$EVENTS"
"$REAL_TAR" "$@"
status=$?
if [ "$status" -eq 0 ] && [ "${MUTATE_AFTER_EXTRACT:-0}" = 1 ]; then
    while [ "$#" -gt 0 ]; do
        if [ "$1" = -C ]; then
            printf 'post-authentication mutation\n' >> "$2/omegon"
            break
        fi
        shift
    done
fi
exit "$status"
EOF
chmod +x "${FAKE_BIN}/tar"

BOOTSTRAP_VERIFIER="${TEST_DIR}/bootstrap-verifier"
cat > "$BOOTSTRAP_VERIFIER" <<'EOF'
#!/bin/sh
printf 'external-verifier\n' >> "$EVENTS"
[ "${VERIFIER_REFUSE:-0}" != 1 ] || exit 1
archive=""
manifest=""
bundle=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --archive) archive="$2"; shift 2 ;;
        --manifest) manifest="$2"; shift 2 ;;
        --bundle) bundle="$2"; shift 2 ;;
        *) shift ;;
    esac
done
[ -f "$archive" ] && [ -s "$manifest" ] && [ "$(cat "$bundle")" = 'trusted fixture bundle' ] || exit 1
: > "$AUTH_MARKER"
if [ "${VERIFIER_RESULT_MALFORMED:-0}" = 1 ]; then
    printf '{"status":"success"}\n'
else
    printf '{"status":"success","diagnostics":[{"code":"release_verified"}]}\n'
fi
EOF
chmod +x "$BOOTSTRAP_VERIFIER"

make_archive 1.0.0 valid
make_archive 2.0.0 valid
make_archive 3.0.0 corrupt
make_archive 4.0.0 missing

install_fixture() {
    version="$1"
    rm -f "$AUTH_MARKER"
    fixture_archive="${FIXTURES}/${version}/omegon-${version}-${TARGET}.tar.gz"
    OMEGON_BOOTSTRAP_VERIFIER="$BOOTSTRAP_VERIFIER" \
    OMEGON_INSTALL_ARCHIVE="$fixture_archive" \
    OMEGON_INSTALL_CHECKSUMS="${FIXTURES}/${version}/checksums.sha256" \
    OMEGON_INSTALL_MANIFEST="${fixture_archive}.manifest.json" \
    OMEGON_INSTALL_BUNDLE="${fixture_archive}.manifest.sigstore.json" \
    HOME="$TEST_HOME" INSTALL_DIR="$INSTALL_DIR" VERSION="v${version}" NO_COLOR=1 \
    TMPDIR="$INSTALLER_TMP" EVENTS="$EVENTS" AUTH_MARKER="$AUTH_MARKER" \
    REAL_TAR="$REAL_TAR" FIXTURE_VERSION="$version" PATH="${FAKE_BIN}:$PATH" \
    sh "${SCRIPT_DIR}/install.sh" --no-confirm
}

assert_selected() {
    version="$1"
    [ "$(EVENTS="$EVENTS" FIXTURE_VERSION="$version" "${INSTALL_DIR}/omegon")" = "omegon ${version}" ]
    [ "$(EVENTS="$EVENTS" FIXTURE_VERSION="$version" "${INSTALL_DIR}/omegon-maintain" --version)" = "omegon-maintain ${version}" ]
    [ "$(FIXTURE_VERSION="$version" "${TEST_HOME}/.omegon/current/share/omegon/extensions/omegon-codescan/target/release/omegon-codescan")" = "codescan ${version}" ]
    grep -q "\"version\": \"v${version}\"" "${TEST_HOME}/.config/omegon/install-receipt.json"
}

assert_no_leaks() {
    refused_version="$1"
    [ ! -e "${TEST_HOME}/.omegon/versions/v${refused_version}" ]
    ! find "${TEST_HOME}/.omegon/versions" -name '*.staging.*' -print -quit 2>/dev/null | grep -q .
    ! find "$INSTALLER_TMP" -mindepth 1 -print -quit | grep -q .
}

assert_no_staging_leaks() {
    ! find "${TEST_HOME}/.omegon/versions" -name '*.staging.*' -print -quit 2>/dev/null | grep -q .
    ! find "$INSTALLER_TMP" -mindepth 1 -print -quit | grep -q .
}

fixture_archive="${FIXTURES}/1.0.0/omegon-1.0.0-${TARGET}.tar.gz"
if OMEGON_INSTALL_ARCHIVE="$fixture_archive" \
    OMEGON_INSTALL_CHECKSUMS="${FIXTURES}/1.0.0/checksums.sha256" \
    OMEGON_INSTALL_MANIFEST="${fixture_archive}.manifest.json" \
    OMEGON_INSTALL_BUNDLE="${fixture_archive}.manifest.sigstore.json" \
    HOME="$TEST_HOME" INSTALL_DIR="$INSTALL_DIR" VERSION=v1.0.0 NO_COLOR=1 \
    TMPDIR="$INSTALLER_TMP" sh "${SCRIPT_DIR}/install.sh" --no-confirm >/dev/null 2>&1; then
    echo 'not ok - valid checksum admitted without an explicit verifier'
    exit 1
fi
[ ! -s "$EVENTS" ]
echo 'ok - valid checksum is insufficient without an explicit external verifier'

if VERIFIER_REFUSE=1 install_fixture 1.0.0 >/dev/null 2>&1; then
    echo 'not ok - external verifier refusal admitted'
    exit 1
fi
[ "$(cat "$EVENTS")" = external-verifier ]
assert_no_leaks 1.0.0
echo 'ok - refusal occurs before extraction or candidate execution and leaves no staging'

: > "$EVENTS"
if VERIFIER_RESULT_MALFORMED=1 install_fixture 1.0.0 >/dev/null 2>&1; then
    echo 'not ok - malformed external verifier success admitted'
    exit 1
fi
[ "$(cat "$EVENTS")" = external-verifier ]
assert_no_leaks 1.0.0
echo 'ok - malformed external verifier output fails before extraction'

: > "$EVENTS"
install_fixture 1.0.0 >/dev/null
assert_selected 1.0.0
[ "$(sed -n '1p' "$EVENTS")" = external-verifier ]
[ "$(sed -n '2p' "$EVENTS")" = extract ]
[ "$(sed -n '3p' "$EVENTS")" = candidate-maintain ]
[ "$(sed -n '4p' "$EVENTS")" = candidate-omegon ]
echo 'ok - exact-tree revalidation precedes other candidate execution'

: > "$EVENTS"
if CANDIDATE_RESULT_MALFORMED=1 install_fixture 2.0.0 >/dev/null 2>&1; then
    echo 'not ok - malformed extracted-tree verifier success admitted'
    exit 1
fi
assert_selected 1.0.0
assert_no_leaks 2.0.0
echo 'ok - malformed extracted-tree evidence preserves the active generation'

: > "$EVENTS"
if MUTATE_AFTER_EXTRACT=1 install_fixture 2.0.0 >/dev/null 2>&1; then
    echo 'not ok - post-authentication extracted-root mutation admitted'
    exit 1
fi
assert_selected 1.0.0
assert_no_leaks 2.0.0
echo 'ok - authenticated maintenance revalidation rejects extracted-root mutation without activation or leaks'

: > "$EVENTS"
if install_fixture 3.0.0 >/dev/null 2>&1; then
    echo 'not ok - corrupt product component lock activated'
    exit 1
fi
assert_selected 1.0.0
assert_no_leaks 3.0.0
echo 'ok - late candidate refusal preserves the active selector and removes candidate staging'

operator_extension="${TEST_HOME}/.omegon/extensions/omegon-codescan"
mkdir -p "$operator_extension"
printf 'operator-owned\n' > "${operator_extension}/sentinel"
if install_fixture 4.0.0 >/dev/null 2>&1; then
    echo 'not ok - partial component generation activated'
    exit 1
fi
assert_selected 1.0.0
[ "$(cat "${operator_extension}/sentinel")" = operator-owned ]
assert_no_leaks 4.0.0
echo 'ok - partial refusal preserves active generation and operator-owned state'

: > "$EVENTS"
if install_fixture 1.0.0 >/dev/null 2>&1; then
    echo 'not ok - pre-existing version directory substituted for authenticated candidate'
    exit 1
fi
assert_selected 1.0.0
assert_no_staging_leaks
echo 'ok - pre-existing version directory is never substituted for authenticated bytes'

grep -q 'destination already contains an operator-managed install' "${SCRIPT_DIR}/../Justfile"
grep -q '\.omegon-release-coupled' "${SCRIPT_DIR}/../Justfile"
echo 'ok - development link keeps explicit operator collision protection'
