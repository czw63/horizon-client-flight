#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
launcher="${HORIZON_LAUNCHER_HOME:-$HOME/.local/share/horizon-launcher}"
rustc="${RUSTC:-$launcher/rust/1.99.1-x86_64-unknown-linux-gnu/bin/rustc}"
deps="$launcher/native/prebuilt-package/deps"
test -x "$rustc"
test -f "$deps/libhorizon_core.so"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export LD_LIBRARY_PATH="$deps:$launcher/native/prebuilt-package/host:$("$rustc" --print target-libdir)${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

"$rustc" --edition=2024 --test src/correction.rs -o "$tmp/correction"
"$tmp/correction"
"$rustc" --edition=2024 --test src/nofall.rs -o "$tmp/nofall"
"$tmp/nofall"
"$rustc" --edition=2024 --test src/flight_velocity_tests.rs -o "$tmp/velocity"
"$tmp/velocity"
"$rustc" --edition=2024 --test tests/probe_geometry_native.rs \
    -C prefer-dynamic -L "dependency=$deps" \
    -L "dependency=$launcher/native/prebuilt-package/host" \
    --extern "horizon_core=$deps/libhorizon_core.so" \
    -o "$tmp/geometry"
"$tmp/geometry"
