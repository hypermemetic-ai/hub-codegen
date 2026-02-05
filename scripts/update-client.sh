#!/usr/bin/env bash
# Generic codegen pipeline for any language
#
# Usage:
#   ./update-client.sh --lang <rust|typescript> --generate <target-repo> [options]
#   ./update-client.sh --lang <rust|typescript> <ir-file> <target-repo>

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
    echo "Usage: $0 --lang <rust|typescript|python> [options]"
    echo ""
    echo "Mode 1: Use existing IR file"
    echo "  $0 --lang <language> <ir-file> <target-repo-dir>"
    echo ""
    echo "Mode 2: Auto-generate IR using synapse"
    echo "  $0 --lang <language> --generate <target-repo-dir> [options]"
    echo ""
    echo "Arguments:"
    echo "  --lang LANG          Target language (rust, typescript, python)"
    echo "  --generate           Generate IR automatically"
    echo "  target-repo-dir      Path to target repository"
    echo ""
    echo "Options:"
    echo "  --host HOST          Substrate host (default: 127.0.0.1)"
    echo "  --port PORT          Substrate port (default: 44410)"
    echo "  --backend NAME       Backend name (default: substrate)"
    echo ""
    echo "Examples:"
    echo "  $0 --lang rust --generate ../substrate-rust-codegen"
    echo "  $0 --lang typescript --generate ../substrate-ts-codegen --port 44410"
    echo "  $0 --lang rust /tmp/ir.json ../substrate-rust-codegen"
}

# Language profile definitions
declare -A LANG_CODEGEN_FLAG=(
    [rust]="rust"
    [typescript]="typescript"
    [python]="python"
)

declare -A LANG_PACKAGE_NAME=(
    [rust]="substrate-client"
    [typescript]="@substrate/client"
    [python]="substrate-client"
)

declare -A LANG_MANIFEST_FILE=(
    [rust]="Cargo.toml"
    [typescript]="package.json"
    [python]="pyproject.toml"
)

# Language-specific codegen command
get_codegen_command() {
    local lang="$1"
    local ir_file="$2"
    local output_dir="$3"

    local flag="${LANG_CODEGEN_FLAG[$lang]}"
    echo "$CODEGEN_BIN \"$ir_file\" -o \"$output_dir\" -t $flag"
}

# Language-specific compilation check
get_check_command() {
    local lang="$1"

    case "$lang" in
        rust)
            echo "cargo check --quiet 2>&1"
            ;;
        typescript)
            echo "npm install --quiet && npm run build 2>&1"
            ;;
        python)
            echo "pip install -e . --quiet && python -m py_compile \$(find . -name '*.py') 2>&1"
            ;;
        *)
            error "Unknown language: $lang"
            ;;
    esac
}

# Language-specific version update
update_version() {
    local lang="$1"
    local manifest="$2"
    local version="$3"

    case "$lang" in
        rust)
            sed -i.bak "s/^version = .*/version = \"$version\"/" "$manifest"
            rm -f "$manifest.bak"
            ;;
        typescript)
            # Use jq to update package.json
            tmp=$(mktemp)
            jq ".version = \"$version\"" "$manifest" > "$tmp"
            mv "$tmp" "$manifest"
            ;;
        python)
            sed -i.bak "s/^version = .*/version = \"$version\"/" "$manifest"
            rm -f "$manifest.bak"
            ;;
        *)
            error "Unknown language: $lang"
            ;;
    esac
}

# Parse arguments
GENERATE_IR=false
IR_FILE=""
TARGET_DIR=""
HOST="127.0.0.1"
PORT="44410"
BACKEND="substrate"
LANGUAGE=""

if [ $# -eq 0 ]; then
    show_usage
    exit 1
fi

# Parse --lang first
while [ $# -gt 0 ]; do
    case "$1" in
        --lang)
            shift
            LANGUAGE="$1"
            shift
            ;;
        --generate)
            GENERATE_IR=true
            shift
            if [ $# -gt 0 ] && [[ ! "$1" =~ ^-- ]]; then
                TARGET_DIR="$1"
                shift
            fi
            ;;
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
        --help|-h)
            show_usage
            exit 0
            ;;
        *)
            # Positional arguments
            if [ -z "$IR_FILE" ] && [ "$GENERATE_IR" = false ]; then
                IR_FILE="$1"
            elif [ -z "$TARGET_DIR" ]; then
                TARGET_DIR="$1"
            else
                error "Unknown argument: $1"
            fi
            shift
            ;;
    esac
done

# Validate language
if [ -z "$LANGUAGE" ]; then
    error "Missing --lang argument"
fi

if [ -z "${LANG_CODEGEN_FLAG[$LANGUAGE]:-}" ]; then
    error "Unsupported language: $LANGUAGE (must be: rust, typescript, python)"
fi

# Validate target directory
if [ -z "$TARGET_DIR" ]; then
    error "Missing target-repo-dir argument"
fi

# Find hub-codegen binary
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CODEGEN_DIR="$(dirname "$SCRIPT_DIR")"
CODEGEN_BIN="$CODEGEN_DIR/target/debug/hub-codegen"

if [ ! -f "$CODEGEN_BIN" ]; then
    CODEGEN_BIN="$CODEGEN_DIR/target/release/hub-codegen"
fi

if [ ! -f "$CODEGEN_BIN" ]; then
    error "hub-codegen binary not found. Run 'cargo build --features $LANGUAGE' first."
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
info "  Language: $LANGUAGE"
if [ "$GENERATE_IR" = true ]; then
    info "  Mode:     Auto-generate IR"
    info "  Source:   $HOST:$PORT/$BACKEND"
else
    info "  Mode:     Use existing IR"
fi
info "  IR file:  $IR_FILE"
info "  Target:   $TARGET_DIR"

# Extract IR hash and version
IR_HASH=$(jq -r '.irHash // "unknown"' "$IR_FILE")
IR_VERSION=$(jq -r '.irVersion // "unknown"' "$IR_FILE")

info "  IR hash:  $IR_HASH"
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

info "Generating ${LANGUAGE} client to temporary directory..."
CODEGEN_CMD=$(get_codegen_command "$LANGUAGE" "$IR_FILE" "$TEMP_DIR")
CODEGEN_OUTPUT=$(eval "$CODEGEN_CMD" 2>&1)
if ! echo "$CODEGEN_OUTPUT" | grep -q "Wrote:"; then
    echo "$CODEGEN_OUTPUT"
    error "Code generation failed"
fi

# Verify generated code compiles
info "Verifying generated code compiles..."
cd "$TEMP_DIR"

CHECK_CMD=$(get_check_command "$LANGUAGE")
if ! eval "$CHECK_CMD" | tail -1 | grep -q -E "(Finished|success|Successfully)"; then
    warn "Generated code has warnings (may be expected)"
fi

# Check for compilation errors
if eval "$CHECK_CMD" 2>&1 | grep -q -E "(error|Error|ERROR)"; then
    error "Generated code does not compile! Aborting."
fi

info "Generated code compiles successfully ✓"

# Determine version number
TIMESTAMP=$(date +%Y%m%d%H%M%S)
SHORT_HASH=${IR_HASH:0:8}
NEW_VERSION="0.1.${TIMESTAMP}-${SHORT_HASH}"

info "New version: $NEW_VERSION"

# Update version in manifest
MANIFEST="${LANG_MANIFEST_FILE[$LANGUAGE]}"
if [ -f "$TEMP_DIR/$MANIFEST" ]; then
    update_version "$LANGUAGE" "$TEMP_DIR/$MANIFEST" "$NEW_VERSION"
fi

# Replace contents of target directory (preserve .git, .gitignore, README.md)
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
COMMIT_MSG="chore: update generated ${LANGUAGE} client to IR hash ${SHORT_HASH}

Generated from IR version ${IR_VERSION}
Full hash: ${IR_HASH}
Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")
Language: ${LANGUAGE}
Generated by: hub-codegen"

# Commit changes
info "Committing changes..."
git commit -m "$COMMIT_MSG"

COMMIT_SHA=$(git rev-parse --short HEAD)
info "Created commit: $COMMIT_SHA"

# Create git tag
TAG_NAME="v${NEW_VERSION}"
info "Creating tag: $TAG_NAME"

git tag -a "$TAG_NAME" -m "Generated ${LANGUAGE} client for IR ${SHORT_HASH}

IR Version: ${IR_VERSION}
Full IR Hash: ${IR_HASH}
Language: ${LANGUAGE}
Generated: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"

info "Tag created: $TAG_NAME"

# Summary
echo ""
info "✓ Pipeline completed successfully!"
echo ""
echo "Summary:"
echo "  Language: $LANGUAGE"
echo "  Commit:   $COMMIT_SHA"
echo "  Tag:      $TAG_NAME"
echo "  Version:  $NEW_VERSION"
echo "  IR Hash:  $IR_HASH"
echo ""
echo "Next steps:"
echo "  git push origin $CURRENT_BRANCH  # Push commit"
echo "  git push origin $TAG_NAME        # Push tag"
