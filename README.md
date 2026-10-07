# hub-codegen

Generates typed Plexus RPC clients — TypeScript and Rust — from synapse's IR.

```
Plexus RPC backend ──schema──▶ synapse (IR builder) ──IR JSON──▶ hub-codegen ──▶ TypeScript / Rust client
```

hub-codegen reads one IR document and writes a client. It never talks to a
backend: synapse builds the IR from a running backend's schema, and
synapse-cc orchestrates a whole run — IR, codegen, merging with files edited
on disk, installing, compiling and smoke-testing the client.

## Running it

**Through synapse-cc** (the normal way). A project made from plexus-starter
runs `scripts/codegen.sh` (behind `npm run build`), which runs
`synapse-cc build` inside the `plexus-codegen` Docker image — synapse,
synapse-cc and hub-codegen in one image, built by
`dockerfiles/plexus-codegen/build.sh` in the hypermemetic workspace. With the tools installed, `synapse-cc build` in a
directory holding a `synapse.config.json` does the same on the host.
synapse-cc calls hub-codegen with `--output-format json` and writes the files
itself.

**Directly**, for development:

```bash
synapse -P <port> -i <backend> > ir.json            # the IR, from a running backend

cargo run -- ir.json -o ./generated                 # TypeScript client (the default target)
cargo run -- ir.json -o ./rust-client -t rust       # Rust client
cargo run -- ir.json --output-format json           # the JSON envelope synapse-cc consumes, on stdout
synapse -P <port> -i <backend> | cargo run -- -o ./generated   # IR on stdin
```

| option | |
| -- | -- |
| `-t, --target` | `typescript` (default) or `rust` |
| `-o, --output` | output directory in `files` mode (default `./generated`) |
| `--output-format` | `files` (write, with three-way merge and a cache manifest) or `json` (stdout, no writes) |
| `--generate` | TS artifact subset: `all`, `transport`, `rpc`, `plugins`, `smoke`, `package` |
| `--plugins` | comma-separated namespaces to generate |
| `--transport` | `ws` (Node), `browser` (native WebSocket) or `none` (external `@plexus/rpc-client`) |
| `--merge-strategy` | what `files` mode does with a file edited since it was generated: `skip` (default) or `force` |
| `--backend-url` | the URL the generated smoke tests fall back to when `PLEXUS_URL` is unset; no default — without it they require `PLEXUS_URL` |
| `--fail-on-deprecated`, `--no-deprecation-annotations` | deprecated IR surface: fail on it, or don't annotate it |

`cargo run -- --help` lists every option.

## Output

TypeScript:

```
generated/
├── types.ts            # protocol types: PlexusStreamItem, StandardRequest/Response, errors
├── rpc.ts              # RpcClient interface and stream helpers
├── transport.ts        # PlexusRpcClient — WebSocket transport, credentials, bidirectional requests
├── index.ts            # re-exports, one `export * as <Namespace>` per namespace
├── package.json, tsconfig.json, .codegen-metadata.json
├── <namespace>/        # nested namespaces nest as directories
│   ├── types.ts
│   ├── client.ts       # create<Namespace>Client(rpc)
│   └── index.ts
└── test/smoke.test.ts  # bun test; plus bidir-smoke.test.ts when the backend has `interactive`
```

Rust:

```
rust-client/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── types.rs
    ├── client.rs
    └── <namespace>/mod.rs
```

Cargo features pick the generators: `typescript` and `rust` (both on by
default), or `all`.

## Using a generated TypeScript client

```typescript
import { createClient, Greeter, type StandardRequest, type StandardResponse } from './generated';

const rpc = createClient({ backend: 'starter', url: process.env.PLEXUS_URL! });
await rpc.connect();
const greeter = Greeter.createGreeterClient(rpc);
```

### Bidirectional requests

During a streaming call the server may send a `request` stream item
(`PlexusStreamItemRequest`: `requestId`, `requestData: StandardRequest`,
`timeoutMs`) and wait for the client's answer. The transport passes
`requestData` to the `onBidirectionalRequest` handler from its config (or
`setBidirectionalHandler`) and sends what it returns to `<backend>.respond`.
No handler, a handler that throws or returns `undefined`, or no answer within
`timeoutMs` sends `{ type: 'cancelled' }`.

```typescript
const rpc = createClient({
  backend: 'starter',
  url: process.env.PLEXUS_URL!,
  onBidirectionalRequest: async (req: StandardRequest): Promise<StandardResponse | undefined> => {
    switch (req.type) {
      case 'confirm': return { type: 'confirmed', value: await askYesNo(req.message, req.default) };
      case 'prompt':  return { type: 'text', value: await ask(req.message, req.default) };
      case 'select':  return { type: 'selected', values: await choose(req.message, req.options, req.multiSelect) };
    }
  },
});
```

| request | fields | response |
| -- | -- | -- |
| `confirm` | `message`, `default?` | `{ type: 'confirmed', value: boolean }` |
| `prompt` | `message`, `default?`, `placeholder?` | `{ type: 'text', value: string }` |
| `select` | `message`, `options: { value, label, description? }[]`, `multiSelect?` | `{ type: 'selected', values: string[] }` |
| any | | `{ type: 'cancelled' }` |

## Development

```bash
cargo build
cargo test        # no backend needed; see tests/README.md
```

## Documentation

[`docs/architecture/`](docs/architecture/):

- [overview](docs/architecture/overview.md) — where hub-codegen sits and its main concepts
- [CLI and pipeline](docs/architecture/cli-and-pipeline.md) — options and code flow
- [IR data model](docs/architecture/ir-data-model.md)
- [TypeScript generator](docs/architecture/typescript-generator.md)
- [Rust generator and testing](docs/architecture/rust-generator-and-testing.md)
- [caching and merge](docs/architecture/caching-and-merge.md),
  [cache contracts](docs/architecture/CACHE_CONTRACTS.md),
  [incremental codegen design](docs/architecture/INCREMENTAL_CODEGEN.md)

The files named `<number>_<topic>.md` there are dated design records from
early 2026, kept as history: the backend, ports and paths they describe are
retired, and the code is the authority where they disagree.
