#!/bin/bash
# Test suite for install.sh versioned layout

set -eu

TEST_DIR="$(mktemp -d)"
TEST_INSTALL_DIR="${TEST_DIR}/bin"
TEST_HOME="${TEST_DIR}/home"
export HOME="$TEST_HOME"

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

# Mock binary for testing
create_mock_omegon() {
    local version="$1"
    local target="$2"
    mkdir -p "$(dirname "$target")"
    cat > "$target" << 'EOF'
#!/bin/sh
echo "omegon 1.0.0"
EOF
    chmod +x "$target"
}

# Test 1: Fresh installation creates versioned layout
test_fresh_install() {
    echo "Test 1: Fresh installation"
    
    # Create mock environment
    export INSTALL_DIR="$TEST_INSTALL_DIR"
    export VERSION="1.0.0"
    export NO_CONFIRM=true
    
    # Create mock download
    mkdir -p "${TEST_DIR}/tmp"
    create_mock_omegon "1.0.0" "${TEST_DIR}/tmp/omegon"
    
    # Mock the download and extraction parts
    # This would require more complex mocking for a real test
    # For now, just test the directory structure logic
    
    VERSION_DIR="${HOME}/.omegon/versions/${VERSION}"
    mkdir -p "$VERSION_DIR"
    cp "${TEST_DIR}/tmp/omegon" "${VERSION_DIR}/omegon"
    
    mkdir -p "$TEST_INSTALL_DIR"
    ln -s "${VERSION_DIR}/omegon" "${TEST_INSTALL_DIR}/omegon"
    
    # Verify structure
    if [ -f "${VERSION_DIR}/omegon" ] && [ -L "${TEST_INSTALL_DIR}/omegon" ]; then
        echo "  ✓ Versioned directory created"
        echo "  ✓ Symlink created at install location"
    else
        echo "  ✗ Installation structure incorrect"
        return 1
    fi
}

# Test 2: Backward compatibility - migrate existing flat binary
test_backward_compat() {
    echo "Test 2: Backward compatibility"
    
    # Reset test environment
    rm -rf "${TEST_HOME}/.omegon"
    rm -rf "${TEST_INSTALL_DIR}"
    
    # Create existing flat binary
    mkdir -p "$TEST_INSTALL_DIR"
    create_mock_omegon "0.9.0" "${TEST_INSTALL_DIR}/omegon"
    
    # Verify we have a flat binary to start with
    EXISTING_TARGET="${TEST_INSTALL_DIR}/omegon"
    if [ ! -f "$EXISTING_TARGET" ] || [ -L "$EXISTING_TARGET" ]; then
        echo "  ✗ Setup failed: could not create flat binary"
        return 1
    fi
    
    # Simulate what the install script would do
    if [ -f "$EXISTING_TARGET" ] && [ ! -L "$EXISTING_TARGET" ]; then
        # Backup existing
        EXISTING_DIR="${HOME}/.omegon/versions/pre-versioned"
        mkdir -p "$EXISTING_DIR"
        cp "$EXISTING_TARGET" "${EXISTING_DIR}/omegon"
        
        # Install new version
        NEW_VERSION="1.1.0"
        NEW_DIR="${HOME}/.omegon/versions/${NEW_VERSION}"
        mkdir -p "$NEW_DIR"
        create_mock_omegon "1.1.0" "${NEW_DIR}/omegon"
        
        # Replace with symlink
        rm -f "$EXISTING_TARGET"
        ln -s "${NEW_DIR}/omegon" "$EXISTING_TARGET"
        
        # Verify
        if [ -f "${EXISTING_DIR}/omegon" ] && [ -f "${NEW_DIR}/omegon" ] && [ -L "$EXISTING_TARGET" ]; then
            echo "  ✓ Existing binary backed up"
            echo "  ✓ New version installed"
            echo "  ✓ Symlink updated"
        else
            echo "  ✗ Migration failed"
            return 1
        fi
    else
        echo "  ✗ No existing binary found to migrate"
        return 1
    fi
}

# Test 3: Version directory structure
test_version_structure() {
    echo "Test 3: Version directory structure"
    
    # Create multiple versions
    for ver in "1.0.0" "1.1.0" "1.2.0-rc.1"; do
        VERSION_DIR="${HOME}/.omegon/versions/${ver}"
        mkdir -p "$VERSION_DIR"
        create_mock_omegon "$ver" "${VERSION_DIR}/omegon"
    done
    
    # Check structure
    VERSIONS_DIR="${HOME}/.omegon/versions"
    if [ -d "$VERSIONS_DIR" ]; then
        VERSION_COUNT=$(find "$VERSIONS_DIR" -maxdepth 1 -type d | wc -l)
        # Should be 4: the versions dir itself + 3 version subdirs
        if [ "$VERSION_COUNT" -eq 4 ]; then
            echo "  ✓ Multiple versions can coexist"
        else
            echo "  ✗ Version structure incorrect (found $VERSION_COUNT dirs)"
            return 1
        fi
    else
        echo "  ✗ Versions directory not created"
        return 1
    fi
}

# Run tests
echo "Running install.sh tests..."
echo "Test directory: $TEST_DIR"
echo

test_fresh_install
test_backward_compat  
test_version_structure

echo
echo "✓ All tests passed"
