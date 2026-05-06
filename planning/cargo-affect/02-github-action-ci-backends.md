# GitHub Action and CI Backend Recipes

Depends on: Foundation CLI and Graph Planner

## Purpose

Make the tool easy to adopt in CI. WarpBuild is the first optimized backend because `runpod-jobs` already uses WarpBuild runners and `WarpBuilds/rust-cache@v2`, but the planner output must stay backend-neutral so GitHub-hosted runners and Blacksmith can be added later.

The design rule: `cargo-affect` emits package sets, matrix data, and cache dimensions. Backend recipes translate those generic outputs into provider-specific runner labels and cache actions.

The install rule: CI should not rebuild `cargo-affect` just to decide what changed. The action should prefer a prebuilt release binary, then a restored binary cache, and only fall back to `cargo install` when no binary is available.

## Step 1: Add Action Wrapper

Files:

- `action.yml`
- `scripts/install.sh`

Inputs:

- `workspace`
- `base`
- `config`
- `set`

Outputs:

- `packages`
- `package-args`
- `nextest-expr`
- `json`
- `empty`
- `cache-group`
- `backend`
- `runner`

Install behavior:

- Resolve host OS/arch to a release artifact name.
- Download the pinned release binary when the action runs from a tag.
- Restore a tool-cache entry when available.
- Fall back to `cargo install --locked --path .` only for local development or unreleased SHAs.

## Step 2: Add CI Backend Recipes

Files:

- `README.md`
- `.github/workflows/ci.yml` for this repo

Add a backend-neutral example first:

- Planner step using `cargo affect plan`.
- Test step consuming `nextest-expr` by default, with `package-args` available for non-nextest Cargo commands.
- Cache key dimensions based on toolchain, OS, target triple, profile, feature mode, workspace root, and package set.

Add a WarpBuild recipe:

- `actions/checkout@v4` with `fetch-depth: 0`.
- `WarpBuilds/rust-cache@v2` with stable `shared-key`.
- `cargo nextest run -E '${{ steps.affect.outputs.nextest-expr }}'`.

Document future backend slots:

- `github`: GitHub-hosted runner labels and `actions/cache` or `Swatinem/rust-cache`.
- `blacksmith`: Blacksmith runner labels and cache action once the exact interface is chosen.
- `warpbuild`: WarpBuild runner labels and `WarpBuilds/rust-cache`.

Do not hard-code backend choices in the core planner.

## Step 3: Add Release/Install Docs

Document:

- `cargo install cargo-affect`
- `cargo install --git https://github.com/lightsofapollo/cargo-affect`
- using the action from a pinned tag.
- why CI should use the prebuilt binary path by default.

## Validation

Run:

```bash
cargo fmt -- --check
cargo nextest run
```

Manually inspect rendered README and action output contract.
