# service-codegen

Generates typed Plexus RPC clients — TypeScript and Rust — from synapse's IR.

```
Plexus RPC service ──schema──▶ synapse (IR builder) ──IR JSON──▶ service-codegen ──▶ TypeScript / Rust client
```

service-codegen reads one IR document and writes a client. It never talks to a
backend: synapse builds the IR from a running service's schema, and
synapse-cc orchestrates a whole run — IR, codegen, merging with files edited
on disk, installing, compiling and smoke-testing the client.

## Naming

A Plexus backend is a *service* (owner, 2026-10-29; NAMING-1), so this tool
is `service-codegen` from 0.5.1 on (NAMING-10): the repository, the crate
(`service_codegen`), the binary and every generated header. For one release
the old binary name is a symlink to this one, and the JSON it shares with
synapse-cc reads the old keys and writes only the new: the output's
`serviceCodegenVersion`, the cache manifest's `"service-codegen"` and its
directory `~/.cache/plexus-codegen/service-codegen/` (an old cache is read
when no new one exists). NAMING-18 removes the old names, each marked
`naming-alias` in the source. `scripts/naming-gate.sh` (run by
`tests/naming_gate.rs`, so `cargo test` and CI) fails on any new remnant.

## Running it

**Through synapse-cc** (the normal way). A project made from plexus-starter
runs `scripts/codegen.sh` (behind `npm run build`), which runs
`synapse-cc build` inside the `plexus-codegen` Docker image — synapse,
synapse-cc and service-codegen in one image, built by
`dockerfiles/plexus-codegen/build.sh` in the hypermemetic workspace. With the tools installed, `synapse-cc build` in a
directory holding a `synapse.config.json` does the same on the host.
synapse-cc calls service-codegen with `--output-format json` and writes the files
itself.

**Directly**, for development:

```bash
synapse -P <port> -i <backend> > ir.json            # the IR, from a running service

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

It reads IR 2.0 (synapse ≤ 3.16), 2.1 (synapse 3.17) and 3.0 (RETURNS-13:
each method's item, error, progress and cardinality), and refuses any other
version before it writes anything. A method generates by its cardinality —
one item is a `Promise<T>`, many a `MethodStream<'ns.m', T>` — whichever
version said it.

## Output

TypeScript:

```
generated/
├── types.ts            # protocol types: PlexusStreamItem, errors
├── rpc.ts              # RpcClient interface and stream helpers
├── transport.ts        # PlexusRpcClient — WebSocket transport, credentials
├── index.ts            # re-exports, one `export * as <Namespace>` per namespace
├── package.json, tsconfig.json, .codegen-metadata.json
├── <namespace>/        # nested namespaces nest as directories
│   ├── types.ts
│   ├── client.ts       # create<Namespace>Client(rpc)
│   └── index.ts
└── test/smoke.test.ts  # bun test
```

A method's parameters are the ones the caller sends. A parameter the server
injects — `pdSource` with any `from` but `rpc` (`auth`, `derived`, `header`,
`cookie`, `query`) — is in neither the signature nor the params object; its
JSDoc breadcrumb (`@requiresAuth — resolver: …`, `@reads-header origin`,
`@server-derived auth`) says where it comes from (SYNAPSE-CC-2; before 0.5.0
it was an optional `unknown` argument). A static child (`mdRole:
static_child`) is a typed sub-client: `tickets.inputs.answer(…)`.

A type the protocol root declares (namespace `""` in an IR 2.0) and a
namespace uses is generated in the namespace that refers to it most — a tie
to the shortest name, then the first — and imported by the others; the
root's own types are the transport's and are not generated (TICKETS-37).

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

Injected parameters are left out the same way and listed in the function's
doc comment ("Injected by the server, never sent"); a static child is its
module (`tickets::inputs::answer(&client, …)`), not a function.

Cargo features pick the generators: `typescript` and `rust` (both on by
default), or `all`.

## Using a generated TypeScript client

```typescript
import { createClient, Greeter } from './generated';

const rpc = createClient({ backend: 'starter', url: process.env.PLEXUS_URL! });
await rpc.connect();
const greeter = Greeter.createGreeterClient(rpc);
```

### No server-to-client requests

A server never asks its caller mid-call (ASK-1): the bidirectional `request`
item, `<backend>.respond` and `onBidirectionalRequest` are gone, and an old
server's `request` item is neither answered nor yielded. An older IR's
`bidirectional` is read and ignored. A service that wants a caller's
confirmation builds it as its own methods (a `dry_run`, a plan id).

## Development

```bash
cargo build
cargo test        # no backend needed; see tests/README.md
```

## Documentation

[`docs/architecture/`](docs/architecture/):

- [overview](docs/architecture/overview.md) — where service-codegen sits and its main concepts
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
