#!/bin/sh
# Builds the tab's native module, tabs/editor.wasm, from native/: what the
# manifest's `native` names and the native Hangar runs. Hangar never builds a
# kit, so the module is committed with the source that made it: run this in the
# commit that changes native/.
#
# Needs Rust; native/rust-toolchain.toml names the version and the
# wasm32-unknown-unknown target, which rustup installs the first time. The
# paths of this machine are taken out of the module, so that it is the same
# byte for byte wherever it is built: CI builds it again and compares.
set -eu
kit="$(cd "$(dirname "$0")/.." && pwd)"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
cd "$kit/native"
RUSTFLAGS="--remap-path-prefix=$kit=/kit --remap-path-prefix=$cargo_home=/cargo" \
  cargo build --release --locked --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/kit_editor_tab.wasm "$kit/tabs/editor.wasm"
echo "tabs/editor.wasm"
