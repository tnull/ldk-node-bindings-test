#!/usr/bin/env bash
[[ $(uname -s) = Linux && $(uname -m) =~ ^(x86_64|aarch64)$ ]] || { echo 'JVM assembly requires Linux x86_64 or aarch64' >&2; exit 1; }
platform=jvm
# work is supplied by build-inner.sh, which sources this script.
# shellcheck disable=SC2154
project="$work/jvm"
cp -R bindings/kotlin/ldk-node-jvm "$project"
chmod -R u+w "$project"
cp -R "$work/generated/kotlin/jvm/." "$project/lib/src/main/kotlin/"
resources="$project/lib/src/main/resources"
# candidate is supplied by build-inner.sh.
# shellcheck disable=SC2154
for target in linux-x86_64 linux-aarch64 darwin-x86_64 darwin-aarch64; do
    extension=so
    [[ $target = darwin-* ]] && extension=dylib
    destination="$resources/${target/x86_64/x86-64}"
    mkdir -p "$destination"
    cp "$candidate/jvm-$target.$extension" "$destination/libldk_node.$extension"
done
source scripts/build-gradle.sh
