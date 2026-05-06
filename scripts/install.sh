#!/usr/bin/env bash
set -euo pipefail

host_os="$(uname -s | tr '[:upper:]' '[:lower:]')"
host_arch="$(uname -m)"

case "$host_os:$host_arch" in
  linux:x86_64) target="linux-x86_64" ;;
  linux:aarch64|linux:arm64) target="linux-aarch64" ;;
  darwin:x86_64) target="macos-x86_64" ;;
  darwin:arm64) target="macos-aarch64" ;;
  *)
    echo "unsupported cargo-affect host: $host_os/$host_arch" >&2
    exit 1
    ;;
esac

version="${CARGO_AFFECT_VERSION:-}"
if [ -z "$version" ] && [[ "${CARGO_AFFECT_ACTION_REF:-}" == v* ]]; then
  version="$CARGO_AFFECT_ACTION_REF"
fi

action_path="${CARGO_AFFECT_ACTION_PATH:-$(pwd)}"
install_root="${RUNNER_TOOL_CACHE:-$HOME/.cache/cargo-affect}/cargo-affect/${version:-local}/$target"
bin_path="$install_root/cargo-affect"
github_path="${GITHUB_PATH:-/dev/null}"
runner_temp="${RUNNER_TEMP:-/tmp}"

if [ -x "$bin_path" ]; then
  echo "$install_root" >> "$github_path"
  echo "cargo-affect restored from tool cache: $bin_path"
  exit 0
fi

mkdir -p "$install_root"

if [ -n "$version" ]; then
  asset="cargo-affect-$target.tar.gz"
  url="https://github.com/lightsofapollo/cargo-affect/releases/download/$version/$asset"
  archive="$runner_temp/$asset"

  if curl --fail --location --silent --show-error "$url" --output "$archive"; then
    tar -xzf "$archive" -C "$install_root"
    chmod +x "$bin_path"
    echo "$install_root" >> "$github_path"
    echo "cargo-affect installed from release: $url"
    exit 0
  fi

  echo "release binary unavailable, falling back to local build: $url" >&2
fi

cargo build --release --locked --manifest-path "$action_path/Cargo.toml"
cp "$action_path/target/release/cargo-affect" "$bin_path"
chmod +x "$bin_path"
echo "$install_root" >> "$github_path"
echo "cargo-affect built from source fallback: $bin_path"
