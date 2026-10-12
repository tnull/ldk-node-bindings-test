#!/usr/bin/env bash
# Linux-compatible orchestration test; does not emulate Xcode or dyld.
set -euo pipefail
scratch=$(mktemp -d /tmp/ldk-apple-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/source/src" "$scratch/source/macro/src" "$scratch/hostile/recipe/scripts" "$scratch/hostile/.cargo"
cp scripts/build-darwin.sh scripts/build-swift-metadata.sh "$scratch/hostile/recipe/scripts/"
cp release.json "$scratch/hostile/recipe/"
mkdir -p "$scratch/hostile/recipe/bindings"
cp -R bindings/swift "$scratch/hostile/recipe/bindings/"
printf '[build]\nrustc = "/unwanted/parent/compiler"\n' > "$scratch/hostile/.cargo/config.toml"
cat > "$scratch/source/Cargo.toml" <<'TOML'
[package]
name = "cargo-location-probe"
version = "0.0.0"
edition = "2021"
[dependencies]
macro-probe = { path = "macro" }
[profile.release-smaller]
inherits = "release"
opt-level = "z"
lto = true
TOML
cat > "$scratch/source/macro/Cargo.toml" <<'TOML'
[package]
name = "macro-probe"
version = "0.0.0"
edition = "2021"
[lib]
proc-macro = true
TOML
printf '#[proc_macro]\npub fn probe(input: proc_macro::TokenStream) -> proc_macro::TokenStream { input }\n' > "$scratch/source/macro/src/lib.rs"
printf 'macro_probe::probe! { pub fn probe() {} }\n' > "$scratch/source/src/lib.rs"
set +e
(
    set -euo pipefail
    cd "$scratch/hostile/recipe"
    export DEVELOPER_DIR=/nix/store/unwanted-sdk SDKROOT=/nix/store/unwanted-sdk
    export NIX_CFLAGS_COMPILE=unwanted NIX_LDFLAGS=unwanted
    export LDK_XCODE_DEVELOPER_DIR=/Applications/ChosenXcode.app/Contents/Developer
    export RUSTFLAGS='' CFLAGS='' CARGO_TARGET_DIR="$scratch/target"
    work=$scratch
    uname() { if [[ $1 = -s ]]; then echo Darwin; else echo arm64; fi; }
    function /usr/bin/xcodebuild() {
        [[ $DEVELOPER_DIR = "$LDK_XCODE_DEVELOPER_DIR" && ! -v SDKROOT && ! -v NIX_CFLAGS_COMPILE && ! -v NIX_LDFLAGS ]] || {
            echo 'Apple tools still use the Nix SDK environment' >&2; return 1;
        }
        printf 'Xcode 27.0\nBuild version 27A266a\n'
    }
    function /usr/bin/xcrun() {
        if [[ $3 = --show-sdk-version ]]; then
            jq -r --arg sdk "$2" '.tools.apple_sdks[$sdk]' release.json
        else
            printf '/Applications/ChosenXcode.app/SDKs/%s.sdk\n' "$2"
        fi
    }
    cargo() {
        # Real Cargo must discover configuration from the scratch source, not
        # from a checkout whose parent contains a hostile Cargo configuration.
        local profile host
        host=$(rustc -vV | sed -n 's/^host: //p')
        for profile in release release-smaller; do
            RUSTFLAGS='' CARGO_TARGET_DIR="$work/target-$profile" command cargo build \
                --offline --profile "$profile" --target "$host" \
                --manifest-path "$work/source/Cargo.toml" -vv > "$work/compiler.log" 2>&1 || {
                cat "$work/compiler.log"
                echo 'Cargo failed in the isolated source directory' >&2; return 1;
            }
            # Host procedural macros do not receive target RUSTFLAGS. Inspect
            # real compiler arguments to catch Cargo's implicit debug stripping.
            grep -- '--crate-type proc-macro' "$work/compiler.log" > "$work/macro.log"
            if grep -Eq -- 'strip=(debuginfo|symbols)' "$work/macro.log"; then
                echo "Cargo strips host procedural macros in $profile" >&2; return 1
            fi
        done
        printf 'reached clean native compiler\n' > "$scratch/passed"
        return 73
    }
    source scripts/build-darwin.sh
) > "$scratch/build.log" 2>&1
status=$?
set -e
cat "$scratch/build.log"
[[ $status = 73 && -f $scratch/passed ]] || exit 1
