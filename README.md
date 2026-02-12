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

## Docker

Run the full pipeline (Substrate → Synapse → hub-codegen) in Docker:

```bash
# Build image
docker build -t hub-codegen:dev .

# Run full pipeline (mounts substrate and synapse source)
docker-compose run dev

# Or specify language
docker-compose run dev typescript
```

See [README.docker.md](README.docker.md) for details.

## Bidirectional Communication (Client-Side)

Plexus supports **bidirectional communication**, where servers can request input from clients during stream execution. This section documents how TypeScript clients handle these requests.

### Overview

During a streaming RPC call, the server may send `PlexusStreamItem_Request` items that require a client response:

```
Client                              Server
  |                                    |
  |---- call("wizard", {}) ----------->|
  |                                    |
  |<--- PlexusStreamItem_Data ---------|  (WizardEvent::Started)
  |                                    |
  |<--- PlexusStreamItem_Request ------|  (prompt: "Enter name")
  |---- respond(requestId, text) ----->|
  |                                    |
  |<--- PlexusStreamItem_Data ---------|  (WizardEvent::NameCollected)
  |<--- PlexusStreamItem_Done ---------|
```

### PlexusStreamItem_Request Format

When the server needs client input, it sends a request item:

```typescript
interface PlexusStreamItem_Request {
  type: 'request';
  requestId: string;           // UUID to correlate response
  requestData: StandardRequest; // The actual request
  timeoutMs: number;           // How long server will wait
}
```

### StandardRequest Types

Three standard request types cover common UI patterns:

```typescript
// Confirmation (yes/no)
interface StandardRequest_Confirm {
  type: 'confirm';
  message: string;      // "Delete 3 files?"
  default?: boolean;    // Suggested default
}

// Text input
interface StandardRequest_Prompt {
  type: 'prompt';
  message: string;      // "Enter project name:"
  default?: string;     // Pre-filled value
  placeholder?: string; // Input hint
}

// Selection menu
interface StandardRequest_Select {
  type: 'select';
  message: string;      // "Choose template:"
  options: SelectOption[];
  multiSelect?: boolean;
}

interface SelectOption {
  value: string;        // Returned when selected
  label: string;        // Display text
  description?: string; // Additional context
}
```

### StandardResponse Types

Respond with the matching type:

```typescript
// Response to confirm
interface StandardResponse_Confirmed {
  type: 'confirmed';
  value: boolean;
}

// Response to prompt
interface StandardResponse_Text {
  type: 'text';
  value: string;
}

// Response to select
interface StandardResponse_Selected {
  type: 'selected';
  values: string[];  // Selected option values
}

// Cancel any request
interface StandardResponse_Cancelled {
  type: 'cancelled';
}
```

### Handling Requests (WebSocket Transport)

The `PlexusRpcClient` accepts a bidirectional handler in its config:

```typescript
import { createClient, StandardRequest, StandardResponse } from './generated';

const client = createClient({
  backend: 'substrate',
  url: 'ws://localhost:4444',
  onBidirectionalRequest: async (request: StandardRequest): Promise<StandardResponse | undefined> => {
    switch (request.type) {
      case 'confirm':
        // Show confirmation dialog
        const confirmed = await showConfirmDialog(request.message);
        return { type: 'confirmed', value: confirmed };

      case 'prompt':
        // Show text input
        const text = await showPromptDialog(request.message, request.default);
        if (text === null) return { type: 'cancelled' };
        return { type: 'text', value: text };

      case 'select':
        // Show selection menu
        const selected = await showSelectDialog(
          request.message,
          request.options,
          request.multiSelect
        );
        if (selected === null) return { type: 'cancelled' };
        return { type: 'selected', values: selected };

      default:
        return { type: 'cancelled' };
    }
  }
});

// Now calls to bidirectional methods will trigger the handler
const stream = client.interactive.wizard();
for await (const item of stream) {
  // Process stream items as usual
  // Requests are handled automatically by onBidirectionalRequest
}
```

### Handling Requests (MCP Transport)

For MCP transport, requests arrive as logging notifications and responses
are sent via the `_plexus_respond` tool:

```typescript
// Request arrives as logging notification
{
  "method": "notifications/message",
  "params": {
    "level": "warning",
    "logger": "plexus",
    "data": {
      "type": "request",
      "requestId": "550e8400-e29b-41d4-a716-446655440000",
      "requestData": {
        "type": "confirm",
        "message": "Delete files?"
      },
      "timeoutMs": 30000
    }
  }
}

// Respond by calling _plexus_respond tool
{
  "method": "tools/call",
  "params": {
    "name": "_plexus_respond",
    "arguments": {
      "request_id": "550e8400-e29b-41d4-a716-446655440000",
      "response": {
        "type": "confirmed",
        "value": true
      }
    }
  }
}
```

### Timeout Behavior

- Requests include `timeoutMs` indicating how long the server will wait
- If the client doesn't respond in time, the server receives a timeout error
- Clients should cancel requests they can't handle immediately

### Example: CLI Implementation

```typescript
import * as readline from 'readline';

async function handleBidirectionalRequest(
  request: StandardRequest
): Promise<StandardResponse> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
  });

  try {
    switch (request.type) {
      case 'confirm': {
        const answer = await question(rl, `${request.message} (y/n): `);
        const value = answer.toLowerCase().startsWith('y');
        return { type: 'confirmed', value };
      }

      case 'prompt': {
        const defaultHint = request.default ? ` [${request.default}]` : '';
        const answer = await question(rl, `${request.message}${defaultHint}: `);
        return { type: 'text', value: answer || request.default || '' };
      }

      case 'select': {
        console.log(request.message);
        request.options.forEach((opt, i) => {
          const desc = opt.description ? ` - ${opt.description}` : '';
          console.log(`  ${i + 1}. ${opt.label}${desc}`);
        });
        const answer = await question(rl, 'Selection: ');
        const index = parseInt(answer) - 1;
        if (index >= 0 && index < request.options.length) {
          return { type: 'selected', values: [request.options[index].value] };
        }
        return { type: 'cancelled' };
      }
    }
  } finally {
    rl.close();
  }
}

function question(rl: readline.Interface, prompt: string): Promise<string> {
  return new Promise(resolve => rl.question(prompt, resolve));
}
```

### Generated Types Location

The bidirectional types are generated in `types.ts`:

```
generated/
├── types.ts           # Contains PlexusStreamItem_Request, StandardRequest, etc.
├── transport.ts       # PlexusRpcClient with bidirectional support
└── ...
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
