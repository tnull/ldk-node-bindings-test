#!/usr/bin/env bash
[[ $(uname -s) = Linux && $(uname -m) = x86_64 ]] || { echo 'JVM assembly requires Linux x86_64' >&2; exit 1; }
platform=jvm
# work is supplied by build-inner.sh, which sources this script.
# shellcheck disable=SC2154
project="$work/jvm"
cp -R bindings/kotlin/ldk-node-jvm "$project"
chmod -R u+w "$project"
cp -R "$work/generated/kotlin/jvm/." "$project/lib/src/main/kotlin/"
resources="$project/lib/src/main/resources"
mkdir -p "$resources/linux-x86-64" "$resources/darwin-x86-64" "$resources/darwin-aarch64"
# candidate is supplied by build-inner.sh.
# shellcheck disable=SC2154
cp "$candidate/jvm-linux-x86_64.so" "$resources/linux-x86-64/libldk_node.so"
cp "$candidate/jvm-darwin-x86_64.dylib" "$resources/darwin-x86-64/libldk_node.dylib"
cp "$candidate/jvm-darwin-aarch64.dylib" "$resources/darwin-aarch64/libldk_node.dylib"
source scripts/build-gradle.sh
