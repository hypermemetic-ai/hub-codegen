#!/usr/bin/env bash
# Start substrate server for development

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SUBSTRATE_DIR="$(cd "$PROJECT_ROOT/../plexus-substrate" && pwd)"

PORT="${1:-4444}"
LOG_FILE="${2:-/tmp/substrate-dev.log}"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Starting Substrate Development Server"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  Port:     $PORT"
echo "  Log file: $LOG_FILE"
echo "  Binary:   $SUBSTRATE_DIR/target/debug/substrate"
echo ""

# Check if substrate is built
if [ ! -f "$SUBSTRATE_DIR/target/debug/substrate" ]; then
    echo "❌ Substrate not built. Building now..."
    cd "$SUBSTRATE_DIR"
    cargo build
    echo "✓ Substrate built successfully"
    echo ""
fi

# Kill any existing substrate on this port
if lsof -i ":$PORT" > /dev/null 2>&1; then
    echo "⚠ Port $PORT is in use. Killing existing process..."
    pkill -f "substrate.*$PORT" || true
    sleep 2
fi

# Start substrate
echo "Starting substrate on port $PORT..."
"$SUBSTRATE_DIR/target/debug/substrate" --port "$PORT" > "$LOG_FILE" 2>&1 &
SUBSTRATE_PID=$!

# Wait for startup
sleep 2

# Check if it's running
if ps -p $SUBSTRATE_PID > /dev/null; then
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo " ✓ Substrate Started Successfully"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "  WebSocket: ws://localhost:$PORT"
    echo "  MCP HTTP:  http://localhost:$((PORT + 1))/mcp"
    echo "  PID:       $SUBSTRATE_PID"
    echo ""
    echo "Commands:"
    echo "  View logs:   tail -f $LOG_FILE"
    echo "  Stop:        kill $SUBSTRATE_PID"
    echo "  Quick stop:  pkill -f 'substrate.*$PORT'"
    echo ""
    echo "To keep this process running, run:"
    echo "  disown $SUBSTRATE_PID"
    echo ""
else
    echo ""
    echo "❌ Substrate failed to start. Check logs:"
    echo "   tail -50 $LOG_FILE"
    echo ""
    exit 1
fi
