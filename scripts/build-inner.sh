#!/usr/bin/env bash
set -euo pipefail
[[ ${LDK_BUILD_SHELL:-} = 1 ]] || { echo 'Use scripts/build.sh to enter the pinned Nix environment' >&2; exit 1; }
bash scripts/build-inputs.sh
group='' output='' candidate=''
while (($#)); do
    case "$1" in
        --group|--output|--candidate)
            (($# >= 2)) || { echo "Missing value for $1" >&2; exit 1; }
            case "$1" in --group) group=$2;; --output) output=$2;; --candidate) candidate=$2;; esac
            shift 2;;
        *) echo "Unknown argument: $1" >&2; exit 1;;
    esac
done
jq -e --arg group "$group" '.groups | has($group)' release.json >/dev/null || { echo 'Unknown build group' >&2; exit 1; }
[[ -n $output ]] || { echo '--output is required' >&2; exit 1; }
mkdir -p "$output"
output=$(realpath "$output")
[[ -z $(find "$output" -mindepth 1 -print -quit) ]] || { echo 'Output directory must be empty' >&2; exit 1; }
[[ -z $candidate ]] || candidate=$(realpath "$candidate")
if [[ $group = jvm && -z $candidate ]]; then echo '--candidate is required for JVM assembly' >&2; exit 1; fi
if [[ ${RELEASE_VERIFY:-} = 1 && -n ${CARGO_TARGET_DIR:-} ]]; then
    parent=$(dirname -- "$CARGO_TARGET_DIR")
    mkdir -p "$parent"
    work=$(mktemp -d "$parent/native-build-XXXXXXXX")
else
    work=$(mktemp -d /tmp/ldk-bindings-build-XXXXXXXX)
fi
cleanup() {
    status=$?
    if [[ $status != 0 && ${RELEASE_VERIFY:-} = 1 ]]; then
        echo "Failed build workspace retained at $work" >&2
    else
        rm -rf "$work"
    fi
    exit "$status"
}
trap cleanup EXIT
# Every invocation starts clean: dependency/tool caches are allowed, project
# generation, native objects, Gradle outputs, and packaging are never reused.
export CARGO_TARGET_DIR="$work/cargo-target"
export CARGO_HOME="$work/cargo-home"
export GRADLE_USER_HOME="$work/gradle-home"
export LC_ALL=C TZ=UTC
export SOURCE_DATE_EPOCH
SOURCE_DATE_EPOCH=$(jq -r '.native.source_date_epoch' release.json)
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR"
cp -R "$LDK_NATIVE_SOURCE" "$work/source"
chmod -R u+w "$work/source"
cat > "$CARGO_HOME/config.toml" <<CFG
[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "$LDK_NATIVE_DEPS"
[net]
offline = true
CFG
export RUSTFLAGS="--remap-path-prefix=$work=/build --remap-path-prefix=$LDK_NATIVE_DEPS=/vendor -C debuginfo=0"
export CFLAGS="-ffile-prefix-map=$work=/build -ffile-prefix-map=$LDK_NATIVE_DEPS=/vendor"
# All platform packages consume the same sandboxed generator output. Rebuild
# it even when present in the store so verification never reuses project work.
nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file .#generated --no-link 2>&1 | tee "$work/generation.log"
generated=$(nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file --rebuild .#generated --no-link --print-out-paths 2> >(tee -a "$work/generation.log" >&2))
mkdir -p "$work/generated"
tar -xf "$generated/generated-sources.tar" -C "$work/generated"
case "$group" in
    generated)
        cp "$generated/generated-sources.tar" "$output/generated-sources.tar";;
    linux-x86_64|linux-aarch64) source scripts/build-linux.sh;;
    android) source scripts/build-android.sh;;
    darwin) source scripts/build-darwin.sh;;
    jvm) source scripts/build-jvm.sh;;
esac
jq -r --arg group "$group" '.groups[$group][]' release.json | sort > "$work/expected"
find "$output" -type f -printf '%f\n' | sort > "$work/actual"
diff -u "$work/expected" "$work/actual"
