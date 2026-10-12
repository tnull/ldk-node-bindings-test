#!/usr/bin/env bash
# Sourced by build-inner.sh; compilation uses only the pinned manylinux sysroot.
[[ $(uname -s) = Linux ]] || { echo 'Linux build requires Linux' >&2; exit 1; }
# group is supplied by build-inner.sh.
# shellcheck disable=SC2154
arch=${group#linux-}
[[ $(uname -m) = "$arch" ]] || { echo "Expected a native $arch runner" >&2; exit 1; }
image=$(jq -r --arg arch "$arch" '.tools.manylinux[$arch]' release.json)
engine=${CONTAINER_ENGINE:-docker}
command -v "$engine" >/dev/null || { echo 'Docker or CONTAINER_ENGINE=podman is required' >&2; exit 1; }
# Host clients may need helpers such as newuidmap or credential helpers. Keep
# these host search paths scoped to the container client, outside compilation.
run_container() {
    PATH="$PATH:$(dirname "$engine"):/usr/bin:/bin:/usr/sbin:/sbin" "$engine" "$@"
}
user_namespace=()
case "$(run_container --version)" in
    *[Pp]odman*) user_namespace=(--userns=keep-id) ;;
esac
run_container pull "$image"
# The container supplies GCC for C compilation and the linker driver/sysroot.
# Rust 1.95 supplies rust-lld on x86_64; no Nix compiler wrapper is used.
# work is supplied by build-inner.sh; quoted variables expand in the container.
# shellcheck disable=SC2154,SC2016
run_container run --rm --network none --read-only --tmpfs /tmp:exec \
    --user "$(id -u):$(id -g)" "${user_namespace[@]}" \
    -v /nix/store:/nix/store:ro -v "$work:/work" \
    -v "$CARGO_TARGET_DIR:/tmp/cargo-target" \
    -e "LDK_RUST=$LDK_RUST" -e SOURCE_DATE_EPOCH \
    -e "CARGO_HOME=/work/cargo-home" -e "CARGO_TARGET_DIR=/tmp/cargo-target" \
    -e "RUSTFLAGS=--remap-path-prefix=/work=/build --remap-path-prefix=/tmp/cargo-target=/build/target --remap-path-prefix=$LDK_NATIVE_DEPS=/vendor -C debuginfo=0" \
    -e "CFLAGS=-ffile-prefix-map=/work=/build -ffile-prefix-map=/tmp/cargo-target=/build/target -ffile-prefix-map=$LDK_NATIVE_DEPS=/vendor" \
    -e 'LC_ALL=C' -e 'TZ=UTC' -w /work/source "$image" /bin/bash -euc '
        export PATH="$LDK_RUST/bin:/opt/rh/gcc-toolset-14/root/usr/bin:/usr/local/bin:/usr/bin:/bin"
        export CC=gcc CXX=g++ AR=ar
        cargo build --frozen --offline --release --no-default-features --features uniffi --lib
        cargo build --frozen --offline --profile release-smaller --no-default-features --features uniffi --lib
        for profile in release release-smaller; do
            if readelf -d /tmp/cargo-target/$profile/libldk_node.so | grep /nix/store; then
                echo "Nix runtime path leaked into distributable library" >&2; exit 1
            fi
        done
        ldd /tmp/cargo-target/release/libldk_node.so
    '
# output is supplied by build-inner.sh.
# shellcheck disable=SC2154
if [[ $arch = x86_64 ]]; then cp "$CARGO_TARGET_DIR/release/libldk_node.so" "$output/jvm-linux-x86_64.so"; fi
source scripts/build-wheel.sh "$work/python" "$CARGO_TARGET_DIR/release-smaller/libldk_node.so" "manylinux_2_28_$arch" "$output"
# Import inside manylinux with its oldest supported Python, outside Nix.
run_container run --rm --network none --read-only --tmpfs /tmp:exec -v "$output:/wheels:ro" "$image" /bin/bash -euc '
    /opt/python/cp310-cp310/bin/python -m venv /tmp/consumer
    /tmp/consumer/bin/pip install --no-index /wheels/*.whl
    /tmp/consumer/bin/python -c "import ldk_node; ldk_node.default_config()"
    auditwheel show /wheels/*.whl
'
