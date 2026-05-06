# Cargo Affect

`cargo-affect` is a Cargo subcommand for planning the smallest useful Rust workspace validation set from a git diff. It is aimed at large workspaces where unrelated crate families live side by side, such as GPU and Virgil crates in `runpod-jobs`.

The core product promise:

- Use `cargo metadata` as the source of truth for workspace packages and dependency edges.
- Map changed files to workspace packages.
- Select changed packages plus all reverse workspace dependents.
- Fail open when a change cannot be mapped safely.
- Emit outputs that are easy to feed into Cargo, cargo-nextest, GitHub Actions, and pluggable CI/cache backends.
- Install fast in CI by using prebuilt binaries by default; rebuilding the planner itself should be a fallback, not the hot path.
- Explain why each package was selected.

## Phases

1. Foundation CLI and Graph Planner
   - Build the cargo subcommand, git diff reader, Cargo metadata graph, reverse dependency closure, and core output formats.

2. Policy Config
   - Add `affect.toml` for global-impact paths, path-to-package mappings, package excludes, and named package sets.

3. GitHub Action and CI Backend Recipes
   - Add an action wrapper and CI examples that use prebuilt binaries, produce a matrix, and produce stable cache keys for WarpBuild first, without coupling the model to WarpBuild.

## Dependency Graph

```text
Foundation CLI and Graph Planner
  -> Policy Config
  -> GitHub Action and CI Backend Recipes
```

Policy and GitHub Action work can happen in parallel after the foundation CLI exists.

## Success Criteria

- `cargo affect packages --workspace <path> --base <ref>` prints affected package names.
- `cargo affect package-args` prints `-p <name>` pairs for direct use in Cargo commands.
- `cargo affect nextest-expr` prints a valid nextest package filter expression.
- CI examples optimize for `cargo nextest run -E '<expr>'`; plain `cargo test` is treated as a fallback path.
- `cargo affect explain` shows selected packages and selection reasons.
- Unknown/global changes can safely select all workspace packages.
- Unit/integration tests cover changed package detection, reverse dependent traversal, unmapped file handling, and Cargo subcommand invocation.
- README documents local usage and backend-friendly GitHub Actions patterns, with WarpBuild as the first recipe and GitHub-hosted/Blacksmith left as straightforward additions.
- GitHub Action install path avoids rebuilding `cargo-affect` on every CI run by downloading a release binary or restoring a binary cache.

## Plan Reference

This folder is the source of truth for Beads issues:

- `planning/cargo-affect/README.md`
- `planning/cargo-affect/architecture.md`
- `planning/cargo-affect/00-foundation-cli.md`
- `planning/cargo-affect/01-policy-config.md`
- `planning/cargo-affect/02-github-action-ci-backends.md`
