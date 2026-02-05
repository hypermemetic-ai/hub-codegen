#!/usr/bin/env bash
# Convenience wrapper for Rust client generation
# Calls the generic update-client.sh with --lang rust

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

exec "$SCRIPT_DIR/update-client.sh" --lang rust "$@"
