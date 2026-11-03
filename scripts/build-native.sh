#!/bin/sh
# Builds the tab's native module, tabs/editor.wasm, from native/: what the
# manifest's tab `entry` names and Hangar runs. Hangar never builds a
# kit, so the module is committed with the source that made it: run this in the
# commit that changes native/.
#
# Needs Rust; native/rust-toolchain.toml names the version and the
# wasm32-unknown-unknown target, which rustup installs the first time. The
# paths of this machine are taken out of the module, so that it is the same
# byte for byte wherever it is built: CI builds it again and compares.
#
#   scripts/build-native.sh           build it, and write tabs/editor.wasm
#   scripts/build-native.sh --check   build it, and fail if tabs/editor.wasm is not what the source builds
set -eu
kit="$(cd "$(dirname "$0")/.." && pwd)"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
cd "$kit/native"
RUSTFLAGS="--remap-path-prefix=$kit=/kit --remap-path-prefix=$cargo_home=/cargo" \
  cargo build --release --locked --target wasm32-unknown-unknown
built=target/wasm32-unknown-unknown/release/kit_editor_tab.wasm
if [ "${1:-}" = "--check" ]; then
  if cmp -s "$built" "$kit/tabs/editor.wasm"; then
    echo "tabs/editor.wasm is what native/ builds."
  else
    echo "tabs/editor.wasm is stale: run scripts/build-native.sh and commit it." >&2
    exit 1
  fi
else
  cp "$built" "$kit/tabs/editor.wasm"
  echo "tabs/editor.wasm"
fi
