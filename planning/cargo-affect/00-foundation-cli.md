# Foundation CLI and Graph Planner

Depends on: none

## Purpose

Ship the reusable core: a Cargo subcommand that uses git changes and Cargo metadata to produce affected workspace package outputs.

## Step 1: Define CLI and Output Types

Files:

- `src/main.rs`

Implement the command surface:

- `packages`
- `package-args`
- `nextest-expr`
- `explain`
- `plan`

Shared options:

- `--workspace <path>` default `.`
- `--base <ref>` default `origin/main`
- `--changed-file <path>` repeatable test/integration override

## Step 2: Implement Workspace Graph Planning

Files:

- `src/main.rs` initially; split later only if needed.

Use `cargo_metadata` to load workspace package paths and dependencies. Compute reverse dependency closure from changed package roots.

Rules:

- A changed file maps to the deepest workspace package directory that contains it.
- Changed package is selected with reason `changed: <file>`.
- Reverse dependents are selected with reason `depends on <package>`.
- Unmapped files select all packages with reason `global impact: <file>`.

## Step 3: Add Tests

Files:

- `src/main.rs` unit tests are acceptable for the first slice.

Test with small temporary git/Cargo workspaces:

- Direct crate change selects that crate and dependents.
- Leaf crate change selects only that crate.
- Workspace-level change selects all.
- `package-args`, `nextest-expr`, and JSON plan formats are stable.
- Cargo subcommand invocation tolerates the leading `affect` argument.

## Validation

Run:

```bash
cargo fmt -- --check
cargo test
cargo run -- packages --changed-file src/main.rs
cargo run -- affect packages --changed-file src/main.rs
```

Expected result: commands pass and local self-change selects `cargo-affect`.
