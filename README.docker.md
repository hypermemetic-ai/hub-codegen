# Docker Development Setup

Simple Docker setup for developing with Substrate, Synapse, and hub-codegen.

## Quick Start

Assuming you have this directory structure:

```
hypermemetic/
├── substrate/       # Substrate source
├── synapse/         # Synapse source
└── hub-codegen/     # This repo
```

### Build the image

```bash
docker build -t hub-codegen:dev .
```

### Run the full pipeline

```bash
# Using docker-compose (easiest)
docker-compose run dev

# Or with docker directly
docker run -v $(pwd)/../substrate:/workspace/substrate \
           -v $(pwd)/../synapse:/workspace/synapse \
           -v $(pwd)/output:/workspace/output \
           hub-codegen:dev dev rust
```

This will:
1. Build substrate (if needed)
2. Build synapse (if needed)
3. Start substrate
4. Use synapse to fetch IR
5. Generate Rust client code
6. Save output to `./output`

## Available Commands

### `dev [lang]` - Full pipeline

```bash
docker-compose run dev        # Generate Rust client
docker-compose run dev rust   # Generate Rust client
docker-compose run dev typescript  # Generate TypeScript client
```

### `substrate` - Run substrate server

```bash
docker run -p 44410:44410 \
           -v $(pwd)/../substrate:/workspace/substrate \
           hub-codegen:dev substrate
```

### `synapse` - Run synapse CLI

```bash
docker run -v $(pwd)/../synapse:/workspace/synapse \
           --network host \
           hub-codegen:dev synapse -P 44410 substrate -i > ir.json
```

### `generate` - Run hub-codegen

```bash
docker run -v $(pwd)/ir.json:/tmp/ir.json \
           -v $(pwd)/output:/workspace/output \
           hub-codegen:dev generate /tmp/ir.json -o /workspace/output -t rust
```

### `bash` - Interactive shell

```bash
docker-compose run shell

# Or
docker run -it \
           -v $(pwd)/../substrate:/workspace/substrate \
           -v $(pwd)/../synapse:/workspace/synapse \
           hub-codegen:dev bash
```

## Directory Structure

```
/workspace/
├── substrate/          # Your substrate source (mounted)
├── synapse/            # Your synapse source (mounted)
├── hub-codegen/        # Built into image
└── output/             # Generated code (mounted)
```

## Building from Source

The Dockerfile builds hub-codegen at image build time. Substrate and synapse are built on first run if needed.

To rebuild everything:

```bash
# Rebuild image
docker build --no-cache -t hub-codegen:dev .

# Remove built binaries to force rebuild
rm -rf ../substrate/target/release/substrate
rm -rf ~/.cabal/bin/synapse

# Run again
docker-compose run dev
```

## Development Workflow

```bash
# 1. Make changes to substrate or synapse source
vim ../substrate/src/main.rs

# 2. Remove the binary to force rebuild
rm ../substrate/target/release/substrate

# 3. Run the pipeline
docker-compose run dev

# 4. Check the generated code
ls -la output/
```

## Troubleshooting

**Q: Builds are slow**

A: Mount cargo cache and cabal cache:

```yaml
volumes:
  - ../substrate:/workspace/substrate
  - ../synapse:/workspace/synapse
  - cargo-cache:/root/.cargo/registry
  - cabal-cache:/root/.cabal
```

**Q: Source not found**

A: Adjust paths in docker-compose.yml to match your directory structure.

**Q: Permission errors**

A: The container runs as root. Output files will be owned by root. Either run `sudo chown -R $USER output/` or add a user to the Dockerfile.
