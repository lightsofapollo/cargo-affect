# cargo-affect

Plan affected Rust workspace checks from git changes.

`cargo-affect` maps changed files to Cargo workspace packages, selects those packages plus reverse workspace dependents, and emits outputs for Cargo, cargo-nextest, and CI planners.

## Usage

```bash
cargo affect packages --workspace crates --base origin/main
cargo affect package-args --workspace crates --base origin/main
cargo affect nextest-expr --workspace crates --base origin/main
cargo affect explain --workspace crates --base origin/main
cargo affect plan --workspace crates --base origin/main
```

For scripts and tests, bypass git diff with explicit files:

```bash
cargo affect package-args \
  --workspace crates \
  --changed-file gpu/gpu-core/src/lib.rs
```

## Policy

Add `affect.toml` at the workspace root when conservative defaults need project knowledge:

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

[ci.profiles.gpu-linux]
set = "gpu"
platform = "linux"
backend = "warpbuild"
cache = "gpu-linux"

[[ci.profiles.gpu-linux.tasks]]
id = "build"
stage = "build"
run = "cargo build --verbose {{ package_args }}"

[[ci.profiles.gpu-linux.tasks]]
id = "nextest"
stage = "test"
run = "cargo nextest run --workspace -E '{{ nextest_expr }}' --no-fail-fast"
```

Then scope a plan:

```bash
cargo affect package-args --workspace crates --set gpu
cargo affect plan --workspace crates --profile gpu-linux
cargo affect ci-run --workspace crates --profile gpu-linux --stage test
```

## GitHub Action

```yaml
jobs:
  plan:
    runs-on: ubuntu-latest
    outputs:
      nextest-expr: ${{ steps.affect.outputs.nextest-expr }}
      empty: ${{ steps.affect.outputs.empty }}
      cache-group: ${{ steps.affect.outputs.cache-group }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: lightsofapollo/cargo-affect@v0.2.1
        id: affect
        with:
          workspace: crates
          profile: gpu-linux

  test:
    needs: plan
    if: needs.plan.outputs.empty != 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-nextest
      - run: cargo nextest run -E '${{ needs.plan.outputs.nextest-expr }}'
        working-directory: crates
```

### WarpBuild Recipe

WarpBuild is just one backend recipe. The important part is that the cache key uses stable dimensions rather than exact commit SHAs:

```yaml
- uses: WarpBuilds/rust-cache@v2
  with:
    workspaces: crates/ -> target
    shared-key: rust-${{ runner.os }}-${{ steps.affect.outputs.cache-group }}
```

### Other Backends

- `github`: use GitHub-hosted runners plus `Swatinem/rust-cache` or `actions/cache`.
- `blacksmith`: use Blacksmith runner labels/cache once chosen.
- `warpbuild`: use WarpBuild runner labels and `WarpBuilds/rust-cache`.

The core planner output is the same for all of them.

## Install Path

CI should not rebuild `cargo-affect` just to decide what changed. The GitHub Action prefers a prebuilt release binary, then a restored tool cache, and only falls back to a local release build for unreleased SHAs or local development.

The optimized test path is `cargo nextest run -E '<expr>'`. `package-args` exists for plain Cargo commands, but large workspaces should usually consume `nextest-expr`.

For local installation:

```bash
cargo install cargo-affect
cargo install --git https://github.com/lightsofapollo/cargo-affect
```
