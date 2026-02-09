#!/usr/bin/env bash
# Wait for synapse build to complete and find the binary

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SYNAPSE_DIR="$(cd "$PROJECT_ROOT/../synapse" && pwd)"

MAX_WAIT="${1:-600}"  # 10 minutes default
INTERVAL=5

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Waiting for Synapse Build"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  Directory: $SYNAPSE_DIR"
echo "  Max wait:  ${MAX_WAIT}s"
echo ""

elapsed=0
while [ $elapsed -lt $MAX_WAIT ]; do
    # Check if synapse binary exists
    SYNAPSE_BIN=$(find "$SYNAPSE_DIR/dist-newstyle" -name "synapse" -type f -executable 2>/dev/null | head -1)

    if [ -n "$SYNAPSE_BIN" ]; then
        echo ""
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo " ✓ Synapse Build Complete"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo ""
        echo "  Binary: $SYNAPSE_BIN"
        echo ""
        echo "Test synapse:"
        echo "  $SYNAPSE_BIN --help"
        echo ""
        echo "Connect to substrate:"
        echo "  $SYNAPSE_BIN -P 4444 plexus health check"
        echo ""
        exit 0
    fi

    # Show build progress
    if ps aux | grep -q "[c]abal build"; then
        echo -n "."
        sleep $INTERVAL
        elapsed=$((elapsed + INTERVAL))
    else
        echo ""
        echo "❌ Build process not running. Check if it failed:"
        echo "   cd $SYNAPSE_DIR && cabal build"
        echo ""
        exit 1
    fi
done

echo ""
echo "❌ Timeout waiting for synapse build ($MAX_WAIT seconds)"
echo ""
echo "Check build status:"
echo "  cd $SYNAPSE_DIR"
echo "  cabal build"
echo ""
exit 1
