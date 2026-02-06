# Development Dockerfile for Substrate + Synapse + hub-codegen
# Builds and runs all components from source

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

# Install base dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    pkg-config \
    libssl-dev \
    jq \
    netcat \
    # Haskell dependencies
    ghc \
    cabal-install \
    libgmp-dev \
    zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Update cabal
RUN cabal update

WORKDIR /workspace

# Build hub-codegen
COPY . /workspace/hub-codegen
WORKDIR /workspace/hub-codegen
RUN cargo build --release --features all

# Expect substrate and synapse to be mounted or cloned
# They will be built on first run or can be pre-built

WORKDIR /workspace

# Create simple entrypoint
COPY <<'EOF' /entrypoint.sh
#!/bin/bash
set -e

echo "🚀 Substrate + Synapse + hub-codegen Development Environment"

# Build substrate if source exists
if [ -d "/workspace/substrate" ] && [ ! -f "/workspace/substrate/target/release/substrate" ]; then
    echo "📦 Building substrate..."
    cd /workspace/substrate
    cargo build --release
fi

# Build synapse if source exists
if [ -d "/workspace/synapse" ] && [ ! -f "/root/.cabal/bin/synapse" ]; then
    echo "📦 Building synapse..."
    cd /workspace/synapse
    cabal build
    cabal install
fi

# Run the requested command
case "${1:-help}" in
    substrate)
        echo "🔧 Starting substrate..."
        exec /workspace/substrate/target/release/substrate
        ;;

    synapse)
        echo "📡 Running synapse..."
        shift
        exec synapse "$@"
        ;;

    generate)
        echo "🔨 Generating client..."
        shift
        exec /workspace/hub-codegen/target/release/hub-codegen "$@"
        ;;

    dev)
        echo "🔄 Running full dev pipeline..."
        # Start substrate in background
        /workspace/substrate/target/release/substrate &
        SUBSTRATE_PID=$!

        # Wait for it to be ready
        echo "⏳ Waiting for substrate..."
        for i in {1..30}; do
            if nc -z localhost 44410 2>/dev/null; then
                echo "✓ Substrate ready!"
                break
            fi
            sleep 1
        done

        # Generate IR
        echo "📡 Fetching IR..."
        synapse -P 44410 substrate -i > /tmp/ir.json

        # Generate client
        LANG="${1:-rust}"
        OUTPUT="${2:-/workspace/output}"
        echo "🔨 Generating ${LANG} client..."
        /workspace/hub-codegen/target/release/hub-codegen /tmp/ir.json -o "$OUTPUT" -t "$LANG"

        # Stop substrate
        kill $SUBSTRATE_PID
        echo "✓ Done! Output in $OUTPUT"
        ;;

    bash)
        exec /bin/bash
        ;;

    *)
        echo "Usage: docker run <image> <command>"
        echo ""
        echo "Commands:"
        echo "  substrate    - Run substrate backend"
        echo "  synapse      - Run synapse CLI"
        echo "  generate     - Run hub-codegen"
        echo "  dev [lang]   - Run full pipeline (default: rust)"
        echo "  bash         - Interactive shell"
        ;;
esac
EOF

RUN chmod +x /entrypoint.sh

EXPOSE 44410

ENTRYPOINT ["/entrypoint.sh"]
CMD ["help"]
