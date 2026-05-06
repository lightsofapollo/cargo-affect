# Architecture

`cargo-affect` has three layers.

## CLI

The binary is named `cargo-affect`, which Cargo exposes as `cargo affect`. Cargo invokes subcommands with the subcommand name as the first argument, so the CLI must tolerate both forms:

```bash
cargo-affect packages
cargo affect packages
```

Primary commands:

- `packages`: newline-separated selected packages.
- `package-args`: shell-friendly `-p name -p other` output.
- `nextest-expr`: package filter expression for `cargo nextest run -E`.
- `explain`: human-readable package reasons.
- `plan`: JSON envelope for CI.

## Planner

Planner inputs:

- Workspace root path.
- Git base ref.
- Optional explicit changed files for tests and non-git integrations.

Planner steps:

1. Run `cargo metadata --format-version 1 --no-deps` in the workspace root.
2. Build package records from workspace members.
3. Build reverse dependency edges between workspace packages.
4. Read changed files from `git diff --name-only <base> --`.
5. Map each changed file to the deepest workspace package directory containing the path.
6. Select changed packages.
7. Walk reverse dependency edges to select all workspace dependents.
8. If a changed file is outside all package directories, mark a global-impact reason and select all packages.

## Output Model

The JSON plan should be stable enough for GitHub Actions and independent of any one CI provider:

```json
{
  "workspace_root": "/repo/crates",
  "base": "origin/main",
  "changed_files": ["gpu/gpu-core/src/lib.rs"],
  "packages": ["gpu-core", "gpu-cli"],
  "package_args": "-p gpu-core -p gpu-cli",
  "nextest_expr": "package(gpu-core) | package(gpu-cli)",
  "select_all": false,
  "cache_dimensions": {
    "workspace": "crates",
    "package_group": "gpu-core,gpu-cli"
  },
  "reasons": {
    "gpu-core": ["changed: gpu/gpu-core/src/lib.rs"],
    "gpu-cli": ["depends on gpu-core"]
  }
}
```

Provider-specific integrations, such as WarpBuild runners/cache, GitHub-hosted runners, or Blacksmith runners, should consume this generic plan rather than changing planner semantics.

## Safety

The default is conservative. If `Cargo.toml`, `Cargo.lock`, `.cargo/**`, workflow files, or unmapped files change before policy config exists, select all packages. Later config can narrow this with explicit path mappings.

## Non-Goals for the First Slice

- Per-test coverage selection.
- Rust symbol/call-graph analysis.
- Duration-aware sharding.
- Editing a target repo's CI automatically.
