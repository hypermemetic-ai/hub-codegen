#!/usr/bin/env bash
# Test the full substrate + synapse pipeline

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Testing Substrate + Synapse Pipeline"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Step 1: Ensure substrate is running
echo "Step 1/3: Starting substrate..."
echo ""
"$SCRIPT_DIR/start-substrate.sh" 4444
echo ""

# Step 2: Wait for synapse build
echo "Step 2/3: Waiting for synapse build..."
echo ""
"$SCRIPT_DIR/wait-for-synapse.sh" 600
SYNAPSE_BIN=$(find ../synapse/dist-newstyle -name "synapse" -type f -executable 2>/dev/null | head -1)
echo ""

# Step 3: Test connection
echo "Step 3/3: Testing synapse → substrate connection..."
echo ""
echo "Running: $SYNAPSE_BIN -P 4444 plexus health check"
echo ""

if ! "$SYNAPSE_BIN" -P 4444 plexus health check 2>&1 | head -20; then
    echo ""
    echo "❌ Connection test failed"
    echo ""
    echo "Check substrate logs:"
    echo "  tail -50 /tmp/substrate-dev.log"
    echo ""
    exit 1
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " ✓ Pipeline Test Complete"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Substrate is running on ws://localhost:4444"
echo "Synapse binary: $SYNAPSE_BIN"
echo ""
echo "Try more commands:"
echo "  # List all activations"
echo "  $SYNAPSE_BIN -P 4444 plexus"
echo ""
echo "  # Get substrate schema"
echo "  $SYNAPSE_BIN -P 4444 plexus schema"
echo ""
echo "  # Echo test"
echo "  $SYNAPSE_BIN -P 4444 plexus echo once --message 'Hello!'"
echo ""
