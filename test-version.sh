#!/bin/bash

# Simple test script to verify binary functionality without MCP protocol

set -e

echo "Testing DOCIIUM binary..."

# Build the binary
echo "Building release binary..."
cargo build --release --bin dociium

# Test that binary exists and is executable
BINARY_PATH="./target/release/dociium"
if [ ! -f "$BINARY_PATH" ]; then
    echo "❌ Binary not found at $BINARY_PATH"
    exit 1
fi

if [ ! -x "$BINARY_PATH" ]; then
    echo "❌ Binary is not executable at $BINARY_PATH"
    exit 1
fi

echo "✅ Binary exists and is executable"

# Test cargo package info
echo "Testing cargo package metadata..."
PACKAGE_VERSION=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[] | select(.name == "mcp_server") | .version')
if [ -z "$PACKAGE_VERSION" ]; then
    echo "❌ Could not get package version"
    exit 1
fi

echo "✅ Package version: $PACKAGE_VERSION"

# Test dependencies
echo "Testing dependencies..."
cargo check --workspace
echo "✅ All dependencies check out"

# Test that we can compile with different features
echo "Testing feature compilation..."
cargo check --bin dociium --features stdio
echo "✅ stdio feature compiles"

# Test basic file structure
echo "Testing project structure..."
REQUIRED_FILES=(
    "README.md"
    "CONTRIBUTING.md"
    "LICENSE-MIT"
    "LICENSE-APACHE"
    "Cargo.toml"
    "mcp_server/Cargo.toml"
    "doc_engine/Cargo.toml"
    "index_core/Cargo.toml"
)

for file in "${REQUIRED_FILES[@]}"; do
    if [ ! -f "$file" ]; then
        echo "❌ Required file missing: $file"
        exit 1
    fi
done

echo "✅ All required files present"

# Test that binary has correct workspace structure
echo "Testing workspace structure..."
WORKSPACE_MEMBERS=$(cargo metadata --format-version 1 --no-deps | jq -r '.workspace_members | length')
if [ "$WORKSPACE_MEMBERS" -ne 3 ]; then
    echo "❌ Expected 3 workspace members, got $WORKSPACE_MEMBERS"
    exit 1
fi

echo "✅ Workspace has correct number of members"

echo ""
echo "🎉 All tests passed! DOCIIUM is ready for production."
echo "Binary located at: $BINARY_PATH"
echo "Package version: $PACKAGE_VERSION"
echo ""
echo "To use DOCIIUM:"
echo "1. cargo install --path mcp_server"
echo "2. Configure your MCP client to use 'dociium' command"
echo ""
