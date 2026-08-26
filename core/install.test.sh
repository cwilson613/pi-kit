#!/bin/bash
# Filesystem contract tests for install.sh's versioned-current-v1 layout.

set -eu

TEST_DIR="$(mktemp -d)"
HOME_ROOT="${TEST_DIR}/home/.omegon"
VERSIONS="${HOME_ROOT}/versions"
CURRENT="${HOME_ROOT}/current"
BIN="${TEST_DIR}/bin"
RECEIPT="${TEST_DIR}/config/install-receipt.json"

cleanup() { rm -rf "$TEST_DIR"; }
trap cleanup EXIT

create_generation() {
    version="$1"
    directory="$2"
    mkdir -p "$directory"
    printf '#!/bin/sh\nprintf "%%s\\n" "%s"\n' "$version" > "${directory}/omegon"
    printf '#!/bin/sh\nprintf "%%s\\n" "%s"\n' "$version" > "${directory}/omegon-maintain"
    printf '{"version":"%s"}\n' "$version" > "${directory}/omegon.composition-lock.json"
    printf '{"version":"%s"}\n' "$version" > "${directory}/omegon-maintain.composition-lock.json"
    printf '{"version":"%s","layout":"versioned-current-v1"}\n' "$version" \
        > "${directory}/install-receipt.json"
    chmod +x "${directory}/omegon" "${directory}/omegon-maintain"
}

atomic_link() {
    target="$1"
    link="$2"
    mkdir -p "$(dirname "$link")"
    temp="${link}.tmp.$$"
    rm -f "$temp"
    ln -s "$target" "$temp"
    if ! mv -f -T "$temp" "$link" 2>/dev/null && ! mv -f -h "$temp" "$link" 2>/dev/null; then
        return 1
    fi
}

assert_generation() {
    expected="$1"
    [ "$("${BIN}/omegon")" = "$expected" ]
    [ "$("${BIN}/om")" = "$expected" ]
    [ "$("${BIN}/omegon-maintain")" = "$expected" ]
    [ -f "${CURRENT}/omegon.composition-lock.json" ]
    [ -f "${CURRENT}/omegon-maintain.composition-lock.json" ]
    grep -q "\"version\":\"${expected}\"" "$RECEIPT"
}

mkdir -p "$VERSIONS" "$BIN" "$(dirname "$RECEIPT")"
create_generation "1.0.0" "${VERSIONS}/1.0.0"
atomic_link "${VERSIONS}/1.0.0" "$CURRENT"
atomic_link "${CURRENT}/omegon" "${BIN}/omegon"
atomic_link "${CURRENT}/omegon" "${BIN}/om"
atomic_link "${CURRENT}/omegon-maintain" "${BIN}/omegon-maintain"
atomic_link "${CURRENT}/install-receipt.json" "$RECEIPT"
assert_generation "1.0.0"
echo "ok - initial pair and receipt share current"

STAGING="${VERSIONS}/.2.0.0.staging"
create_generation "2.0.0" "$STAGING"
mv "$STAGING" "${VERSIONS}/2.0.0"
assert_generation "1.0.0"
echo "ok - publication does not activate candidate"

atomic_link "${VERSIONS}/2.0.0" "$CURRENT"
assert_generation "2.0.0"
[ "$("${VERSIONS}/1.0.0/omegon")" = "1.0.0" ]
echo "ok - one activation switches pair and receipt"

mkdir "${VERSIONS}/incomplete"
printf 'broken\n' > "${VERSIONS}/incomplete/omegon"
if [ -x "${VERSIONS}/incomplete/omegon" ] && \
   [ -x "${VERSIONS}/incomplete/omegon-maintain" ] && \
   [ -f "${VERSIONS}/incomplete/omegon.composition-lock.json" ] && \
   [ -f "${VERSIONS}/incomplete/omegon-maintain.composition-lock.json" ] && \
   [ -f "${VERSIONS}/incomplete/install-receipt.json" ]; then
    echo "not ok - incomplete generation accepted"
    exit 1
fi
assert_generation "2.0.0"
echo "ok - incomplete generation cannot disturb active pair"
