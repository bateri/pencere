#!/bin/sh
# Prints the digest of everything the interpreter module (`kit/sandbox/luau/luau.wasm`) is
# built from: the interface it speaks, the bridge and the preludes, the build script and its pins,
# the generator of the C bindings and the tool that makes the component. `make luau` writes it
# beside the module (`inputs.sha256`) and `make check` compares it, so that a module built from
# sources that have changed since is noticed by the gate and not by the first person to run a
# script.
set -eu
cd "$(dirname "$0")/../.."
export LC_ALL=C
{
	for file in kit/wit/view.wit kit/luau/pins kit/luau/build.sh \
		kit/luau/bridge/* kit/luau/prelude/* \
		kit/guest/cbindgen/src/main.rs kit/guest/componentize/src/main.rs; do
		echo "== $file"
		cat "$file"
	done
	# The releases of the generator and the component tool are lines of the workspace manifest.
	echo "== kit/guest/Cargo.toml"
	grep -E '^wit-' kit/guest/Cargo.toml
} | { shasum -a 256 2>/dev/null || sha256sum; } | cut -d' ' -f1
