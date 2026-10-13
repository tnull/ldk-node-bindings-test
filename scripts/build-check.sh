#!/usr/bin/env bash
# Lightweight release-recipe checks; does not build native LDK libraries.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
for script in scripts/build*.sh; do bash -n "$script"; done
bash scripts/build-swift-metadata.sh
bash scripts/build-check-retention.sh
bash scripts/build-check-environment.sh
bash scripts/build-check-container.sh
bash scripts/build-check-apple.sh
for mode in config archive paths; do bash scripts/build-check-generation.sh "$mode"; done
for mode in swift android; do bash scripts/build-check-metadata.sh "$mode"; done
bash scripts/build-check-jvm.sh
scratch=$(mktemp -d /tmp/ldk-bindings-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
for change in '.profiles.python="release"' '.features=["other"]' '.bindings_version="0.0.0"'; do
    jq "$change" release.json > "$scratch/mismatched.json"
    if bash scripts/build-inputs.sh "$scratch/mismatched.json" > "$scratch/input.log" 2>&1; then
        echo 'Mismatched release input unexpectedly accepted' >&2; exit 1
    fi
    grep -q 'Declared inputs do not match' "$scratch/input.log"
done
if scripts/build.sh --group invalid --output "$scratch/invalid" > "$scratch/invalid.log" 2>&1; then
    echo 'Invalid group unexpectedly accepted' >&2; exit 1
fi
grep -q 'Unknown build group' "$scratch/invalid.log"
mkdir "$scratch/nonempty"
printf 'preserve me\n' > "$scratch/nonempty/existing"
if scripts/build.sh --group generated --output "$scratch/nonempty" > "$scratch/nonempty.log" 2>&1; then
    echo 'Nonempty output unexpectedly accepted' >&2; exit 1
fi
grep -q 'Output directory must be empty' "$scratch/nonempty.log"
grep -q 'preserve me' "$scratch/nonempty/existing"
scripts/build.sh --group generated --output "$scratch/a"
scripts/build.sh --group generated --output "$scratch/b"
cmp "$scratch/a/generated-sources.tar" "$scratch/b/generated-sources.tar"
tar -tf "$scratch/a/generated-sources.tar" > "$scratch/inventory"
for source in swift/Sources/LDKNode/LDKNode.swift swift/LDKNodeFFI.h swift/LDKNodeFFI.modulemap python/ldk_node.py kotlin/jvm/org/lightningdevkit/ldknode/ldk_node.kt kotlin/android/org/lightningdevkit/ldknode/ldk_node.kt; do
    grep -Fxq "./$source" "$scratch/inventory"
done
printf 'Build arguments, output preservation, source coverage, and independent source reproduction passed.\n'
