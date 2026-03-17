# LIVE-GRAPH: Runtime-Augmentable Graphs + Plan Node

Implement LIVE-GRAPH-1. Full plan at:
`/workspace/hypermemetic/plexus-substrate/plans/LIVE-GRAPH/LIVE-GRAPH-1.md`

All source files live under `/workspace/hypermemetic/plexus-substrate/src/`.
Working directory for all agents: `/workspace/hypermemetic/plexus-substrate`

Before making changes, read the relevant source files. After making changes,
verify the file compiles by checking for obvious syntax errors.

---

# L1-SCHEMA: Lattice parent_graph_id column + storage methods [agent]

Read these files before making changes:
- `src/activations/lattice/types.rs`
- `src/activations/lattice/storage.rs`
- `src/activations/lattice/activation.rs`

## 1. `lattice/types.rs` — `LatticeGraph` struct

Add `parent_graph_id: Option<String>` field to `LatticeGraph`:

```rust
pub struct LatticeGraph {
    pub id: GraphId,
    pub metadata: Value,
    pub status: GraphStatus,
    pub created_at: i64,
    pub node_count: usize,
    pub edge_count: usize,
    pub parent_graph_id: Option<String>,   // ← new
}
```

## 2. `lattice/storage.rs` — migration + new methods

### Migration

In `run_migrations`, after the existing `CREATE TABLE` statements, add:

```sql
ALTER TABLE lattice_graphs ADD COLUMN parent_graph_id TEXT NULL REFERENCES lattice_graphs(id);
CREATE INDEX IF NOT EXISTS idx_lattice_graphs_parent ON lattice_graphs(parent_graph_id);
```

Wrap in a check so it's idempotent (like the existing `ALTER TABLE` migrations):
```rust
let _ = sqlx::query(
    "ALTER TABLE lattice_graphs ADD COLUMN parent_graph_id TEXT NULL REFERENCES lattice_graphs(id)"
).execute(&self.pool).await;
let _ = sqlx::query(
    "CREATE INDEX IF NOT EXISTS idx_lattice_graphs_parent ON lattice_graphs(parent_graph_id)"
).execute(&self.pool).await;
```

### `create_child_graph` method

```rust
pub async fn create_child_graph(
    &self,
    parent_id: &str,
    metadata: Value,
) -> Result<String, String>
```

Insert a row into `lattice_graphs` with `parent_graph_id = parent_id`. Return the new graph_id.
Implement by copying `create_graph` and adding the `parent_graph_id` column.

### `get_child_graphs` method

```rust
pub async fn get_child_graphs(&self, parent_id: &str) -> Result<Vec<LatticeGraph>, String>
```

`SELECT ... FROM lattice_graphs WHERE parent_graph_id = ?`. Include `parent_graph_id`
in the SELECT and map it in the row → LatticeGraph conversion.

### Update existing row → `LatticeGraph` mapping

Wherever `lattice_graphs` rows are mapped to `LatticeGraph` structs (in `get_graph`,
`list_graphs`, `get_child_graphs`, and anywhere else), add:

```rust
parent_graph_id: row.try_get::<Option<String>, _>("parent_graph_id").unwrap_or(None),
```

This is safe even before the column exists: `ALTER TABLE` was already run, and
`unwrap_or(None)` handles any transient schema gap.

### `add_node` — live-graph awareness

After the node INSERT, add:

```rust
// Live-graph: if graph is Running, seed this node immediately.
if let Ok(graph_status) = self.get_graph_status(&graph_id).await {
    if graph_status == GraphStatus::Running {
        let _ = self.check_and_ready(&graph_id, &node_id).await;
    }
}
```

`get_graph_status` is a small helper (or inline the query):
```rust
async fn get_graph_status(&self, graph_id: &str) -> Result<GraphStatus, String> {
    let row = sqlx::query("SELECT status FROM lattice_graphs WHERE id = ?")
        .bind(graph_id)
        .fetch_one(&self.pool).await
        .map_err(|e| e.to_string())?;
    let s: String = row.try_get("status").map_err(|e| e.to_string())?;
    s.parse::<GraphStatus>()
}
```

`check_and_ready` already exists for join/gather logic — call it here.
If `check_and_ready` emits a `NodeReady` event, it wakes the execute() stream naturally.

### `add_edge` — live-graph awareness

After the edge INSERT, add:

```rust
// Live-graph: if graph is Running and source is Complete, retroactively deposit tokens.
if let Ok(graph_status) = self.get_graph_status(&graph_id).await {
    if graph_status == GraphStatus::Running {
        // Check if source node is complete and has output tokens
        if let Ok(source_output) = self.get_node_output(&graph_id, &from_node_id).await {
            if let Some(output) = source_output {
                // Deposit the source's tokens on this new edge
                self.deposit_tokens_on_edge(&edge_id, &output).await?;
                // Now check if the target node is ready
                let _ = self.check_and_ready(&graph_id, &to_node_id).await;
            }
        }
    }
}
```

You will need to read the existing `advance_graph` function to understand how it
deposits tokens on edges (it writes to `lattice_edge_tokens`). Replicate that
logic here for the specific new edge. Look for the `deposit` or token-writing
code in `advance_graph`. If a helper doesn't exist, extract one or inline the SQL.

## 3. `lattice/activation.rs`

No structural changes needed. The hub methods (`get_graph`, `list_graphs`) return
`LatticeGraph` structs which now include `parent_graph_id` automatically.

Add two new hub methods at the end of the `Lattice::register` block:

```rust
hub.method("lattice/create_child_graph", |params: CreateChildGraphParams| async move {
    // calls self.storage.create_child_graph(parent_id, metadata)
    // returns CreateResult
});

hub.method("lattice/get_child_graphs", |params: GetChildGraphsParams| async move {
    // calls self.storage.get_child_graphs(parent_id)
    // returns ListGraphsResult
});
```

Define simple param structs (with `parent_id: String`, `metadata: Value`).

---

# L3-TYPES: OrchaNodeKind::Plan + OrchaNodeSpec::Plan [agent]

Read `src/activations/orcha/types.rs` before making changes.

## `OrchaNodeKind`

Add one variant to the existing enum:

```rust
Plan { task: String },
```

The enum uses `#[serde(tag = "orcha_type", rename_all = "snake_case")]` so this
serializes as `{"orcha_type": "plan", "task": "..."}`.

## `OrchaNodeSpec`

Find `OrchaNodeSpec` (used by `run_graph_definition` and the ticket compiler).
Add one variant:

```rust
Plan { task: String },
```

Mirror the existing `Task` variant exactly — same structure, different name.

No other changes needed in this file.

---

# L2-RUNTIME: GraphRuntime.create_child_graph + OrchaGraph.add_plan [agent]
blocked_by: [L1-SCHEMA, L3-TYPES]

Read `src/activations/orcha/graph_runtime.rs` before making changes.

## `GraphRuntime::create_child_graph`

Add after the existing `create_graph` method:

```rust
/// Create a new execution graph as a child of an existing graph.
pub async fn create_child_graph(
    &self,
    parent_id: &str,
    metadata: Value,
) -> Result<OrchaGraph, String> {
    let graph_id = self.storage.create_child_graph(parent_id, metadata).await?;
    Ok(OrchaGraph {
        graph_id,
        storage: self.storage.clone(),
    })
}
```

## `OrchaGraph::add_plan`

Add after the existing `add_synthesize` method:

```rust
/// Add a plan node.
///
/// When dispatched, runs Claude to produce a ticket file, compiles it into
/// a child graph, and executes the child graph inline.
pub async fn add_plan(&self, task: impl Into<String>) -> Result<String, String> {
    let kind = OrchaNodeKind::Plan { task: task.into() };
    self.add_spec(NodeSpec::Task {
        data: serde_json::to_value(&kind).map_err(|e| e.to_string())?,
        handle: None,
    })
    .await
}
```

Make sure `OrchaNodeKind` is in scope (it already is via `use super::types::OrchaNodeKind`).

---

# L4-COMPILER: Add planner ticket type to compiler [agent]
blocked_by: [L3-TYPES]

Read `src/activations/orcha/ticket_compiler.rs` in full before making changes.

Add `[planner]` as a recognized ticket type, mapping to `OrchaNodeSpec::Plan { task }`.

The body parsing is identical to `[agent]`: everything after the metadata lines
(`blocked_by:`, `validate:`) becomes the task string.

Find where `[agent]` is parsed (look for the match on the bracket-tag string).
Add a parallel branch for `"planner"` that produces `OrchaNodeSpec::Plan { task: body }`.

The `blocked_by` and `validate` directives work identically for `[planner]` nodes.

---

# L6-ACTIVATION: cancel_graph recursive + run_graph_definition Plan arm [agent]
blocked_by: [L1-SCHEMA, L3-TYPES]

Read `src/activations/orcha/activation.rs` before making changes.
Focus on: `cancel_graph`, `run_graph_definition`, and `build_graph_from_definition`.

## `cancel_graph` — recursive propagation

Find the `cancel_graph` hub method handler. After sending on the existing watch channel
for `graph_id`, add recursive child cancellation:

```rust
// Recursively cancel child graphs
let mut to_cancel: std::collections::VecDeque<String> = std::collections::VecDeque::new();
to_cancel.push_back(graph_id.clone());

while let Some(gid) = to_cancel.pop_front() {
    // Cancel this graph's watch channel (already done for root above; safe to repeat)
    if let Some(tx) = cancel_registry.get(&gid) {
        let _ = tx.send(true);
    }
    // Enqueue children
    if let Ok(children) = lattice_storage.get_child_graphs(&gid).await {
        for child in children {
            to_cancel.push_back(child.id);
        }
    }
}
```

Read the existing cancel_graph implementation carefully — the above is pseudocode.
Adapt to the actual structure (the registry may be an `Arc<Mutex<HashMap<...>>>` or similar).

## `run_graph_definition` — Plan arm in node-building loop

Find the loop in `run_graph_definition` or `build_graph_from_definition` that matches
on `OrchaNodeSpec` variants to build graph nodes. Add:

```rust
OrchaNodeSpec::Plan { task } => {
    graph.add_plan(&task).await?
}
```

Also find the `dispatch_node` match in graph_runner (or wherever node specs are
dispatched in activation.rs). Add a `Plan` branch that routes to `dispatch_plan`.
Leave `dispatch_plan` as a placeholder (`todo!()` or return an error) — it will be
implemented in L5-RUNNER.

---

# L7-PM: PM recursive graph_status + inspect_ticket child + list_graphs root_only [agent]
blocked_by: [L1-SCHEMA, L3-TYPES]

Read `src/activations/orcha/pm/storage.rs` and `src/activations/orcha/pm/activation.rs`
in full before making changes.

## `list_graphs` — `root_only` parameter

In `pm/activation.rs`, find the `list_graphs` hub method. Add an optional
`root_only: Option<bool>` parameter (default `true`).

In `pm/storage.rs`, add `list_graphs(root_only: bool)` (or update the existing method):

```sql
-- When root_only = true:
SELECT lg.* FROM lattice_graphs lg WHERE lg.parent_graph_id IS NULL ORDER BY lg.created_at DESC

-- When root_only = false:
SELECT lg.* FROM lattice_graphs lg ORDER BY lg.created_at DESC
```

The PM list_graphs calls `lattice_storage.list_graphs()` or queries directly.
Read how it currently works and adapt.

## `graph_status` — `recursive` parameter

In `pm/activation.rs`, find `graph_status`. Add `recursive: Option<bool>` (default `false`).

When `recursive = true` and a ticket's output token contains `"child_graph_id"`:
1. Fetch the child graph's ticket map from PM storage
2. Fetch each child ticket's node status from lattice
3. Embed a `"child_graph"` field in that ticket's entry showing child ticket statuses
4. Apply a depth limit of 3 to prevent runaway recursion

The output token is stored in the lattice node's `output` field. Parse it as JSON
and check for `output.payload.value.child_graph_id`. If present, recurse.

## `inspect_ticket` — child graph embedding

Find `inspect_ticket` in `pm/activation.rs`. After fetching the node and its output,
check if the output token contains `child_graph_id`:

```rust
if let Some(child_graph_id) = output_value.get("child_graph_id").and_then(|v| v.as_str()) {
    // Fetch child graph status
    let child_graph = lattice_storage.get_graph(child_graph_id).await.ok();
    // Fetch child ticket map
    let child_ticket_map = pm_storage.get_ticket_map(child_graph_id).await.unwrap_or_default();
    // For each child ticket, fetch its node status
    // Add "child_graph_status" field to the response
}
```

Include `child_graph_status` in the JSON response for the inspect_ticket hub method.

---

# L5-RUNNER: dispatch_plan + route Plan in dispatch_node + percentage fix [agent/synthesize]
blocked_by: [L2-RUNTIME, L4-COMPILER, L6-ACTIVATION, L7-PM]

Read these files before making changes:
- `src/activations/orcha/graph_runner.rs` (in full)
- `src/activations/orcha/activation.rs` (focus on run_tickets, run_graph_execution call sites,
  cancel_registry structure, build_graph_from_definition)
- `src/activations/orcha/ticket_compiler.rs` (for compile_tickets signature)
- `plans/LIVE-GRAPH/LIVE-GRAPH-1.md` (the full plan for dispatch_plan behavior)

## Fix: percentage tracking

In `run_graph_execution`, change `total_nodes` from a fixed capture to a re-fetched value:

```rust
// Before: let total_nodes: usize = graph.count_nodes().await.unwrap_or(0);
// After: make it mutable and re-fetch on each completion
let mut total_nodes: usize = graph.count_nodes().await.unwrap_or(0);
```

In the completion handler (where `complete_nodes` is incremented), add:
```rust
total_nodes = graph.count_nodes().await.unwrap_or(total_nodes);
```

## `dispatch_plan`

Add a new async function `dispatch_plan`. It runs in four phases:

**Phase 1 — Plan:**
Run a Claude Code session exactly like `dispatch_task`. Collect the full output text.
The node's resolved input tokens are passed as `<prior_work>` context (same as
`dispatch_synthesize`). If the output is empty, fail the node immediately with
`"Planner produced empty output"`.

**Phase 2 — Compile:**
Call `ticket_compiler::compile_tickets(&output_text)`. If compilation fails, fail
the node with a message like:
```
"Planner output did not parse as ticket file. Compile error: {err}\nFirst 200 chars: {&output_text[..200.min(len)]}"
```

**Phase 3 — Build child graph:**
- Call the lattice storage's `create_child_graph(parent_graph_id, metadata)`.
  The `parent_graph_id` is `graph.graph_id`.
- Create an `OrchaGraph` handle for the child.
- Call `build_graph_from_definition` (or equivalent) with the compiled nodes/edges.
- Save the ticket map to PM: `pm.save_ticket_map(&child_graph_id, &id_map)`.

**Phase 4 — Execute child graph:**
- Create a new cancel channel `(child_cancel_tx, child_cancel_rx)`.
- Register `child_cancel_tx` in the cancel registry under `child_graph_id`.
- Call `run_graph_execution` on the child graph with the same model, working_directory,
  and the new `child_cancel_rx`.
- Forward all child `OrchaEvent`s through `output_tx` (the parent's event channel).
- Await child completion.

**Output token:**
On success: `Token::ok_data(serde_json::json!({"child_graph_id": child_graph_id, "summary": &output_text[..200.min(len)]}))`
On failure: return `Err(format!("Child graph failed: {}", error))`

## Wire dispatch_plan into dispatch_node

In the `dispatch_node` function (or wherever `OrchaNodeKind` is matched), add:

```rust
OrchaNodeKind::Plan { task } => {
    dispatch_plan(/* params */, &task).await
}
```

`dispatch_plan` needs more context than `dispatch_task` (it needs graph_runtime/lattice
storage, pm storage, and cancel registry). Thread these through as needed.
The cleanest approach: add them as parameters to `dispatch_node` and `run_graph_execution`,
or define `dispatch_plan` as a closure/method where that context is already in scope.
Read the existing activation.rs to understand where `run_graph_execution` is called
and what's available in that scope.

Do what compiles. If you need to add parameters, add them. Do not use `todo!()` —
implement the full function. The build must pass.

---

# VALIDATE: Build check [prog]
blocked_by: [L5-RUNNER]
validate: cd /workspace/hypermemetic/plexus-substrate && cargo build --package plexus-substrate --features mcp-gateway 2>&1 | tail -3

cd /workspace/hypermemetic/plexus-substrate && cargo build --package plexus-substrate --features mcp-gateway 2>&1 | grep "^error" | head -10; exit 0
