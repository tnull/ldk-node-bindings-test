#!/usr/bin/env bash
# Check upstream-config independence and canonical generated-source consumption.
set -euo pipefail
mode=${1:-config}
scratch=$(mktemp -d /tmp/ldk-generation-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
if [[ $mode = config ]]; then
    cp -R "$LDK_NATIVE_SOURCE" "$scratch/source"
    chmod -R u+w "$scratch/source"
    for variant in missing changed; do
        if [[ $variant = missing ]]; then rm "$scratch/source/uniffi.toml"
        else printf '[bindings.kotlin]\npackage_name="wrong.package"\ncdylib_name="wrong_library"\n' > "$scratch/source/uniffi.toml"; fi
        bash scripts/build-generate.sh "$scratch/source" "$scratch/$variant"
        kotlin="$scratch/$variant/kotlin/android/org/lightningdevkit/ldknode/ldk_node.kt"
        [[ -f $kotlin ]] || { echo 'Android package depends on upstream UniFFI configuration' >&2; exit 1; }
        grep -q '"ldk_node"' "$kotlin"
    done
    cmp "$scratch/missing/kotlin/android/org/lightningdevkit/ldknode/ldk_node.kt" "$scratch/changed/kotlin/android/org/lightningdevkit/ldknode/ldk_node.kt"
    exit
fi
mkdir -p "$scratch/recipe" "$scratch/bin" "$scratch/archive" "$scratch/generated" "$scratch/vendor"
cp -R scripts bindings "$scratch/recipe/"
jq '.groups["linux-x86_64"]=["proof.txt"]' release.json > "$scratch/recipe/release.json"
printf 'canonical archive\n' > "$scratch/generated/sentinel"
tar -cf "$scratch/archive/generated-sources.tar" -C "$scratch/generated" .
printf 'const char *source_file = __FILE__;\n' > "$scratch/vendor/probe.c"
cat > "$scratch/bin/nix" <<'NIX'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$LDK_TEST_ROOT/nix-calls"
if [[ " $* " = *' --print-out-paths '* ]]; then echo "$LDK_TEST_ROOT/archive"; fi
NIX
cat > "$scratch/bin/uniffi-bindgen" <<'GENERATOR'
#!/usr/bin/env bash
if [[ $LDK_TEST_MODE = archive ]]; then echo 'Packaging regenerated wrappers outside the canonical archive' >&2; exit 1; fi
while (($#)); do
    if [[ $1 = --out-dir ]]; then printf 'placeholder\n' > "$2/LDKNode.swift"; exit; fi
    shift
done
GENERATOR
cat > "$scratch/recipe/scripts/build-linux.sh" <<'CONSUMER'
if [[ $LDK_TEST_MODE = archive ]]; then
    grep -Fxq 'canonical archive' "$work/generated/sentinel"
else
    read -r -a flags <<< "$CFLAGS"
    cc -g "${flags[@]}" -c "$LDK_NATIVE_DEPS/probe.c" -o "$work/probe.o"
    if strings "$work/probe.o" | grep -F "$LDK_NATIVE_DEPS"; then
        echo 'C source or debug paths leaked the vendor directory' >&2; exit 1
    fi
    strings "$work/probe.o" | grep -Fq '/vendor/probe.c'
fi
printf 'passed\n' > "$output/proof.txt"
CONSUMER
chmod +x "$scratch/bin/"*
(
    cd "$scratch/recipe"
    PATH="$scratch/bin:$PATH" LDK_TEST_MODE="$mode" LDK_TEST_ROOT="$scratch" \
        LDK_NATIVE_DEPS="$scratch/vendor" bash scripts/build-inner.sh --group linux-x86_64 --output "$scratch/output"
)
grep -Fxq passed "$scratch/output/proof.txt"
if [[ $mode = archive ]]; then grep -q -- '--rebuild' "$scratch/nix-calls"; fi
