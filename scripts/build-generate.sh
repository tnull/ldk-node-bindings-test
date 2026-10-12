#!/usr/bin/env bash
# Called inside the pinned build environment, with a fresh output directory.
set -euo pipefail
source_dir=$1
output=$2
mkdir -p "$output"/kotlin/{jvm,android} "$output/python" "$output/swift/Sources/LDKNode"
uniffi-bindgen generate "$source_dir/bindings/ldk_node.udl" --language kotlin --config bindings/uniffi.toml --out-dir "$output/kotlin/jvm" --no-format
uniffi-bindgen generate "$source_dir/bindings/ldk_node.udl" --language kotlin --config bindings/uniffi-android.toml --out-dir "$output/kotlin/android" --no-format
uniffi-bindgen generate "$source_dir/bindings/ldk_node.udl" --language python --config bindings/uniffi.toml --out-dir "$output/python" --no-format
uniffi-bindgen generate "$source_dir/bindings/ldk_node.udl" --language swift --config bindings/uniffi.toml --out-dir "$output/swift" --no-format
{ printf 'import SystemConfiguration\n'; cat "$output/swift/LDKNode.swift"; } > "$output/swift/Sources/LDKNode/LDKNode.swift"
rm "$output/swift/LDKNode.swift"
