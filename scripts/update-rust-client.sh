#!/usr/bin/env bash
# Codegen pipeline: Generate Rust client, replace existing, commit & tag
#
# Usage:
#   ./update-rust-client.sh <ir-file> <target-repo-dir>
#   ./update-rust-client.sh --generate <target-repo-dir> [--host HOST] [--port PORT] [--backend BACKEND]
#
# Examples:
#   ./update-rust-client.sh /tmp/substrate-ir.json /tmp/plexus-rust-client
#   ./update-rust-client.sh --generate /tmp/plexus-rust-client --port 44410 --backend substrate

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
    exit 1
}

info() {
    echo -e "${GREEN}INFO: $1${NC}"
}

warn() {
    echo -e "${YELLOW}WARN: $1${NC}"
}

show_usage() {
    echo "Usage: $0 <ir-file> <target-repo-dir>"
    echo "   or: $0 --generate <target-repo-dir> [options]"
    echo ""
    echo "Mode 1: Use existing IR file"
    echo "  Arguments:"
    echo "    ir-file          Path to IR JSON file (from synapse -i)"
    echo "    target-repo-dir  Path to target Rust client repository"
    echo ""
    echo "Mode 2: Auto-generate IR using synapse"
    echo "  Arguments:"
    echo "    --generate       Generate IR automatically"
    echo "    target-repo-dir  Path to target Rust client repository"
    echo ""
    echo "  Options:"
    echo "    --host HOST      Substrate host (default: 127.0.0.1)"
    echo "    --port PORT      Substrate port (default: 44410)"
    echo "    --backend NAME   Backend name (default: substrate)"
    echo ""
    echo "Examples:"
    echo "  $0 /tmp/substrate-ir.json /tmp/plexus-rust-client"
    echo "  $0 --generate /tmp/plexus-rust-client"
    echo "  $0 --generate /tmp/plexus-rust-client --port 4444"
}

# Parse arguments
GENERATE_IR=false
IR_FILE=""
TARGET_DIR=""
HOST="127.0.0.1"
PORT="44410"
BACKEND="substrate"

if [ $# -eq 0 ]; then
    show_usage
    exit 1
fi

# Parse command line
if [ "$1" = "--generate" ]; then
    GENERATE_IR=true
    shift

    if [ $# -eq 0 ]; then
        error "Missing target-repo-dir argument"
    fi

    TARGET_DIR="$1"
    shift

    # Parse optional flags
    while [ $# -gt 0 ]; do
        case "$1" in
            --host)
                shift
                HOST="$1"
                shift
                ;;
            --port)
                shift
                PORT="$1"
                shift
                ;;
            --backend)
                shift
                BACKEND="$1"
                shift
                ;;
            *)
                error "Unknown option: $1"
                ;;
        esac
    done
elif [ $# -eq 2 ]; then
    # Mode 1: IR file + target dir
    IR_FILE="$1"
    TARGET_DIR="$2"
else
    show_usage
    exit 1
fi

# Find hub-codegen binary
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CODEGEN_DIR="$(dirname "$SCRIPT_DIR")"
CODEGEN_BIN="$CODEGEN_DIR/target/debug/hub-codegen"

if [ ! -f "$CODEGEN_BIN" ]; then
    CODEGEN_BIN="$CODEGEN_DIR/target/release/hub-codegen"
fi

if [ ! -f "$CODEGEN_BIN" ]; then
    error "hub-codegen binary not found. Run 'cargo build' or 'cargo build --release' first."
fi

# Generate IR if requested
if [ "$GENERATE_IR" = true ]; then
    info "Generating IR from substrate..."
    info "  Host:    $HOST"
    info "  Port:    $PORT"
    info "  Backend: $BACKEND"

    # Check if synapse is available
    if ! command -v synapse &> /dev/null; then
        error "synapse command not found. Install synapse or provide IR file directly."
    fi

    # Create temp file for IR
    IR_FILE=$(mktemp -t substrate-ir.XXXXXX)
    mv "$IR_FILE" "${IR_FILE}.json"
    IR_FILE="${IR_FILE}.json"
    trap "rm -f $IR_FILE" EXIT

    # Generate IR
    if ! synapse -H "$HOST" -P "$PORT" "$BACKEND" -i > "$IR_FILE" 2>&1; then
        error "Failed to generate IR from substrate at $HOST:$PORT"
    fi

    # Validate IR was generated
    if [ ! -s "$IR_FILE" ]; then
        error "Generated IR file is empty"
    fi

    # Validate it's valid JSON
    if ! jq -e . "$IR_FILE" >/dev/null 2>&1; then
        error "Generated IR is not valid JSON"
    fi

    info "IR generated successfully"
fi

# Validate inputs
[ -f "$IR_FILE" ] || error "IR file not found: $IR_FILE"
[ -d "$TARGET_DIR" ] || error "Target directory not found: $TARGET_DIR"
[ -d "$TARGET_DIR/.git" ] || error "Target directory is not a git repository: $TARGET_DIR"

info "Starting codegen pipeline..."
if [ "$GENERATE_IR" = true ]; then
    info "  Mode:    Auto-generate IR"
    info "  Source:  $HOST:$PORT/$BACKEND"
else
    info "  Mode:    Use existing IR"
fi
info "  IR file: $IR_FILE"
info "  Target:  $TARGET_DIR"

# Extract IR hash and version
IR_HASH=$(jq -r '.irHash // "unknown"' "$IR_FILE")
IR_VERSION=$(jq -r '.irVersion // "unknown"' "$IR_FILE")

info "  IR hash: $IR_HASH"
info "  IR version: $IR_VERSION"

# Check if target repo is clean
cd "$TARGET_DIR"
if ! git diff --quiet || ! git diff --cached --quiet; then
    error "Target repository has uncommitted changes. Commit or stash them first."
fi

if [ -n "$(git status --porcelain)" ]; then
    error "Target repository has untracked or modified files. Clean it first."
fi

info "Target repository is clean ✓"

# Get current git branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
info "Current branch: $CURRENT_BRANCH"

# Generate new code to temporary directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

info "Generating Rust client to temporary directory..."
"$CODEGEN_BIN" "$IR_FILE" -o "$TEMP_DIR" -t rust || error "Code generation failed"

# Verify generated code compiles
info "Verifying generated code compiles..."
cd "$TEMP_DIR"
if ! cargo check --quiet 2>&1 | grep -q "Finished"; then
    warn "Generated code has compilation warnings (this is expected for unused imports)"
fi

# Check for compilation errors
if ! cargo check 2>&1 | tail -1 | grep -q "Finished"; then
    error "Generated code does not compile! Aborting."
fi

info "Generated code compiles successfully ✓"

# Determine version number
# Version format: 0.1.<timestamp>-<short-hash>
TIMESTAMP=$(date +%Y%m%d%H%M%S)
SHORT_HASH=${IR_HASH:0:8}
NEW_VERSION="0.1.${TIMESTAMP}-${SHORT_HASH}"

info "New version: $NEW_VERSION"

# Update version in Cargo.toml
sed -i.bak "s/^version = .*/version = \"$NEW_VERSION\"/" "$TEMP_DIR/Cargo.toml"
rm "$TEMP_DIR/Cargo.toml.bak" 2>/dev/null || true

# Replace contents of target directory (preserve .git)
info "Replacing target directory contents..."
cd "$TARGET_DIR"

# Remove everything except .git, .gitignore, and README.md
find . -mindepth 1 -maxdepth 1 ! -name '.git' ! -name '.gitignore' ! -name 'README.md' -exec rm -rf {} +

# Copy new files
cp -r "$TEMP_DIR"/* .

# Stage all changes
git add -A

# Check if there are any changes
if git diff --cached --quiet; then
    info "No changes detected. Client is already up to date."
    exit 0
fi

# Create commit message
COMMIT_MSG="chore: update generated client to IR hash ${SHORT_HASH}

Generated from IR version ${IR_VERSION}
Full hash: ${IR_HASH}
Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")
Generated by: hub-codegen"

# Commit changes
info "Committing changes..."
git commit -m "$COMMIT_MSG"

COMMIT_SHA=$(git rev-parse --short HEAD)
info "Created commit: $COMMIT_SHA"

# Create git tag
TAG_NAME="v${NEW_VERSION}"
info "Creating tag: $TAG_NAME"

git tag -a "$TAG_NAME" -m "Generated client for IR ${SHORT_HASH}

IR Version: ${IR_VERSION}
Full IR Hash: ${IR_HASH}
Generated: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"

info "Tag created: $TAG_NAME"

# Summary
echo ""
info "✓ Pipeline completed successfully!"
echo ""
echo "Summary:"
echo "  Commit:  $COMMIT_SHA"
echo "  Tag:     $TAG_NAME"
echo "  Version: $NEW_VERSION"
echo "  IR Hash: $IR_HASH"
echo ""
echo "Next steps:"
echo "  git push origin $CURRENT_BRANCH  # Push commit"
echo "  git push origin $TAG_NAME        # Push tag"
