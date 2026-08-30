#!/bin/bash
# End-to-end filesystem contract tests for install.sh's full-product generations.

set -eu

TEST_DIR="$(mktemp -d)"
TEST_HOME="${TEST_DIR}/home"
INSTALL_DIR="${TEST_DIR}/bin"
FIXTURES="${TEST_DIR}/fixtures"
FAKE_BIN="${TEST_DIR}/fake-bin"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

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

make_archive() {
    version="$1"
    component_state="$2"
    root="${FIXTURES}/${version}/root"
    archive="${FIXTURES}/${version}/omegon-${version}-${TARGET}.tar.gz"
    mkdir -p "${root}/share/omegon/content-packs/omegon-shipped"
    mkdir -p "${root}/share/omegon/extensions/omegon-codescan/target/release"
    mkdir -p "${root}/share/omegon/components"
    printf '#!/bin/sh\nprintf "omegon %s\\n"\n' "$version" > "${root}/omegon"
    printf '#!/bin/sh\nprintf "omegon-maintain %s\\n"\n' "$version" > "${root}/omegon-maintain"
    printf '#!/bin/sh\nprintf "codescan %s\\n"\n' "$version" \
        > "${root}/share/omegon/extensions/omegon-codescan/target/release/omegon-codescan"
    chmod +x "${root}/omegon" "${root}/omegon-maintain" \
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

    if [ "$component_state" != "missing" ]; then
        manifest="${root}/share/omegon/extensions/omegon-codescan/manifest.toml"
        executable="${root}/share/omegon/extensions/omegon-codescan/target/release/omegon-codescan"
        if [ "$component_state" = "corrupt" ]; then
            printf '{}\n' > "${root}/share/omegon/components/core-codescan.lock.json"
        else
            printf '{"schema_version":1,"component_id":"core:codescan","wire_manifest_id":"omegon-codescan","manifest_path":"share/omegon/extensions/omegon-codescan/manifest.toml","manifest_digest":"%s","executable_path":"share/omegon/extensions/omegon-codescan/target/release/omegon-codescan","executable_digest":"%s","target":"%s","protocol_minimum":1,"protocol_maximum":1,"protocol_version":1,"fallback":"typed_unavailable","signing_identity":{"issuer":"https://token.actions.githubusercontent.com","workflow_identity":"https://github.com/styrene-lab/omegon/.github/workflows/release.yml@refs/tags/v%s","verification":"required"}}\n' \
                "$(sha256 "$manifest")" "$(sha256 "$executable")" "$TARGET" "$version" \
                > "${root}/share/omegon/components/core-codescan.lock.json"
        fi
    fi
    tar czf "$archive" -C "$root" .
    printf '%s  %s\n' "$(sha256 "$archive")" "$(basename "$archive")" \
        > "${FIXTURES}/${version}/checksums.sha256"
}

mkdir -p "$TEST_HOME" "$INSTALL_DIR" "$FIXTURES" "$FAKE_BIN"
make_archive "1.0.0" valid
make_archive "2.0.0" corrupt
make_archive "3.0.0" missing

cat > "${FAKE_BIN}/curl" <<'EOF'
#!/bin/sh
output=""
write_status=false
url=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) output="$2"; shift 2 ;;
        -w) write_status=true; shift 2 ;;
        http*) url="$1"; shift ;;
        *) shift ;;
    esac
done
case "$url" in
    */checksums.sha256) cp "$FIXTURE_CHECKSUMS" "$output" ;;
    *) cp "$FIXTURE_ARCHIVE" "$output" ;;
esac
if [ "$write_status" = true ]; then printf '200'; fi
EOF
chmod +x "${FAKE_BIN}/curl"
cat > "${FAKE_BIN}/xxd" <<EOF
#!/bin/sh
cat >/dev/null
printf '%s\n' '$MAGIC'
EOF
chmod +x "${FAKE_BIN}/xxd"

install_fixture() {
    version="$1"
    FIXTURE_ARCHIVE="${FIXTURES}/${version}/omegon-${version}-${TARGET}.tar.gz" \
    FIXTURE_CHECKSUMS="${FIXTURES}/${version}/checksums.sha256" \
    HOME="$TEST_HOME" INSTALL_DIR="$INSTALL_DIR" VERSION="v${version}" NO_COLOR=1 \
    PATH="${FAKE_BIN}:$PATH" sh "${SCRIPT_DIR}/install.sh" --no-confirm
}

assert_selected() {
    version="$1"
    [ "$("${INSTALL_DIR}/omegon")" = "omegon ${version}" ]
    [ "$("${INSTALL_DIR}/omegon-maintain")" = "omegon-maintain ${version}" ]
    [ "$("${TEST_HOME}/.omegon/current/share/omegon/extensions/omegon-codescan/target/release/omegon-codescan")" = "codescan ${version}" ]
    grep -q "\"version\": \"v${version}\"" "${TEST_HOME}/.config/omegon/install-receipt.json"
}

operator_extension="${TEST_HOME}/.omegon/extensions/omegon-codescan"
mkdir -p "$operator_extension"
printf 'operator-owned\n' > "${operator_extension}/sentinel"

install_fixture "1.0.0" >/dev/null
assert_selected "1.0.0"
[ "$(cat "${operator_extension}/sentinel")" = "operator-owned" ]
echo "ok - complete generation activates without claiming operator extension"

if install_fixture "2.0.0" >/dev/null 2>&1; then
    echo "not ok - corrupt product component lock activated"
    exit 1
fi
assert_selected "1.0.0"
[ "$(cat "${operator_extension}/sentinel")" = "operator-owned" ]
echo "ok - corrupt component verification preserves callable prior generation"

if install_fixture "3.0.0" >/dev/null 2>&1; then
    echo "not ok - partial component generation activated"
    exit 1
fi
assert_selected "1.0.0"
[ "$(cat "${operator_extension}/sentinel")" = "operator-owned" ]
echo "ok - partial staging preserves host, component, receipt, and operator extension"

install_fixture "1.0.0" >/dev/null
assert_selected "1.0.0"
echo "ok - valid rollback restores the complete prior generation"

grep -q 'destination already contains an operator-managed install' "${SCRIPT_DIR}/../Justfile"
grep -q '\.omegon-release-coupled' "${SCRIPT_DIR}/../Justfile"
echo "ok - development link keeps explicit operator collision protection"
