# Policy Config

Depends on: Foundation CLI and Graph Planner

## Purpose

Add an `affect.toml` policy file so real repositories can tune conservative defaults without forking the tool.

## Step 1: Load Config

Files:

- `src/main.rs` or `src/config.rs`

Add:

- `--config <path>` optional.
- Auto-discovery for `affect.toml` in the workspace root.

## Step 2: Implement Policy Rules

Config model:

```toml
global = [
  "Cargo.toml",
  "Cargo.lock",
  ".cargo/**",
  ".github/workflows/**",
]

[paths]
"apps/portal/public/schema/**" = ["gpu-cli", "gpu-core"]
"docs/**" = []

[platform.macos]
exclude = ["relay-manager"]

[sets.gpu]
include = ["gpu-*", "desktop-inspect*", "relay-manager"]
```

Rules:

- `global` path match selects all packages.
- `[paths]` maps non-crate files to packages, or to no packages for docs-only paths.
- Platform excludes remove packages from outputs after selection.
- Sets allow future CI jobs to request `--set gpu` or `--set virgil`.

## Step 3: Add Tests

Cover global rules, path mappings, docs-only no-op paths, excludes, and set filtering.

## Validation

Run:

```bash
cargo fmt -- --check
cargo test
```
