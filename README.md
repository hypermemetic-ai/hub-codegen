# hub-codegen

Multi-language code generator for Plexus clients from Synapse IR.

## Features

The codegen supports multiple target languages via Cargo feature flags:

- `typescript` (default) - Generate TypeScript client
- `rust` - Generate Rust client
- `all` - Enable all generators

## Usage

### TypeScript (default)

```bash
# Generate TypeScript client
synapse plexus -i | cargo run --manifest-path hub-codegen/Cargo.toml -- -o /tmp/client

# Or explicitly
cargo run --features typescript -- -o /tmp/client < ir.json
```

### Rust

```bash
# Generate Rust client
cargo run --features rust --no-default-features -- -t rust -o /tmp/rust-client < ir.json
```

### All Targets

```bash
# Build with all generators
cargo build --features all

# Use CLI flag to select target
cargo run --features all -- -t typescript -o /tmp/ts-client < ir.json
cargo run --features all -- -t rust -o /tmp/rust-client < ir.json
```

## CLI Options

```
Generate client code from Synapse IR

Usage: hub-codegen [OPTIONS] [INPUT]

Arguments:
  [INPUT]  Path to IR JSON file (use - for stdin) [default: -]

Options:
  -o, --output <OUTPUT>  Output directory [default: ./generated]
  -t, --target <TARGET>  Target language [default: typescript] [possible values: typescript, rust]
      --dry-run          Dry run - print generated files without writing
  -h, --help             Print help
```

## Architecture

```
Rust Types → JSON Schema → Synapse IR → hub-codegen → Target Language
                           (Haskell)      (Rust)        (TS/Rust/...)
```

- **IR** (`src/ir.rs`) - Language-agnostic intermediate representation
- **TypeScript Generator** (`src/generator/typescript/`) - TypeScript client generation
- **Rust Generator** (`src/generator/rust/`) - Rust client generation

## Output Structure

### TypeScript
```
generated/
├── types.ts          # Core transport types
├── rpc.ts           # RPC client interface
├── transport.ts     # WebSocket transport
├── index.ts         # Public API
├── package.json
├── tsconfig.json
└── <namespace>/
    ├── types.ts     # Namespace types
    ├── client.ts    # Namespace methods
    └── index.ts     # Namespace exports
```

### Rust
```
generated/
├── lib.rs           # Module re-exports
├── types.rs         # All type definitions
├── client.rs        # PlexusClient with methods
└── Cargo.toml       # Package manifest
```

## Development

```bash
# Check TypeScript codegen
cargo check

# Check Rust codegen
cargo check --features rust --no-default-features

# Check all features
cargo check --features all

# Run tests
cargo test
```
