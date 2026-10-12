#!/usr/bin/env bash
bash scripts/build-swift-metadata.sh
[[ $(uname -s) = Darwin && $(uname -m) = arm64 ]] || { echo 'Darwin group requires an arm64 Mac' >&2; exit 1; }
# Nix's stdenv SDK hook sets these for its own compiler. Native releases use
# the caller-selected Xcode, or the host's xcode-select default.
unset DEVELOPER_DIR SDKROOT NIX_CFLAGS_COMPILE NIX_LDFLAGS
if [[ -n ${LDK_XCODE_DEVELOPER_DIR:-} ]]; then export DEVELOPER_DIR=$LDK_XCODE_DEVELOPER_DIR; fi
# Rust discovers Apple SDKs by invoking xcrun from PATH. Override just xcrun;
# GNU archive and file tools must retain precedence over their BSD namesakes.
# work is supplied by build-inner.sh, which sources this script.
# shellcheck disable=SC2154
mkdir -p "$work/apple-tools"
ln -s /usr/bin/xcrun "$work/apple-tools/xcrun"
export PATH="$work/apple-tools:$PATH"
xcode=$(jq -r .tools.xcode release.json)
xcode_build=$(jq -r .tools.xcode_build release.json)
[[ $(/usr/bin/xcodebuild -version) = "$(printf 'Xcode %s\nBuild version %s' "$xcode" "$xcode_build")" ]] || { echo "Xcode $xcode build $xcode_build is required" >&2; exit 1; }
for sdk in macosx iphoneos iphonesimulator; do
    [[ $(/usr/bin/xcrun --sdk "$sdk" --show-sdk-version) = "$(jq -r --arg sdk "$sdk" '.tools.apple_sdks[$sdk]' release.json)" ]] || { echo "Unexpected $sdk SDK" >&2; exit 1; }
done
export ZERO_AR_DATE=1
export CC=/usr/bin/clang CXX=/usr/bin/clang++ AR=/usr/bin/ar
export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/usr/bin/clang
export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/clang
export CARGO_TARGET_AARCH64_APPLE_IOS_LINKER=/usr/bin/clang
export CARGO_TARGET_X86_64_APPLE_IOS_LINKER=/usr/bin/clang
export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER=/usr/bin/clang
swift_flags=$RUSTFLAGS
export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-install_name,@rpath/libldk_node.dylib"
check_dylib() {
    local library=$1
    /usr/bin/otool -l "$library" | grep 'cmd LC_UUID' >/dev/null || {
        echo "Darwin library is missing LC_UUID: $library" >&2; exit 1;
    }
    [[ $(/usr/bin/otool -D "$library" | tail -n +2) = '@rpath/libldk_node.dylib' ]] || {
        echo "Unstable Darwin library install name: $library" >&2; exit 1;
    }
    if /usr/bin/otool -L "$library" | tail -n +2 | grep /nix/store; then
        echo 'Nix runtime dependency in Darwin library' >&2; exit 1
    fi
}
base_flags=$CFLAGS
for target in x86_64-apple-darwin aarch64-apple-darwin; do
    export SDKROOT
    SDKROOT=$(/usr/bin/xcrun --sdk macosx --show-sdk-path)
    export CFLAGS="$base_flags -ffile-prefix-map=$SDKROOT=/apple-sdk"
    if [[ $target = x86_64-* ]]; then deployment=python_macos_x86_64; wheel_arch=x86_64; arch=x86_64
    else deployment=python_macos_arm64; wheel_arch=arm64; arch=aarch64; fi
    export MACOSX_DEPLOYMENT_TARGET
    MACOSX_DEPLOYMENT_TARGET=$(jq -r --arg deployment "$deployment" '.deployment_targets[$deployment]' release.json)
    platform="macosx_${MACOSX_DEPLOYMENT_TARGET//./_}_$wheel_arch"
    (cd "$work/source" && cargo build --frozen --offline --release --no-default-features --features uniffi --target "$target" --lib)
    library="$CARGO_TARGET_DIR/$target/release/libldk_node.dylib"
    check_dylib "$library"
    # output is supplied by build-inner.sh.
    # shellcheck disable=SC2154
    cp "$library" "$output/jvm-darwin-$arch.dylib"
    (cd "$work/source" && cargo build --frozen --offline --profile release-smaller --no-default-features --features uniffi --target "$target" --lib)
    library="$CARGO_TARGET_DIR/$target/release-smaller/libldk_node.dylib"
    check_dylib "$library"
    source scripts/build-wheel.sh "$work/python-$arch" "$library" "$platform" "$output"
done
export RUSTFLAGS=$swift_flags
export MACOSX_DEPLOYMENT_TARGET IPHONEOS_DEPLOYMENT_TARGET
MACOSX_DEPLOYMENT_TARGET=$(jq -r .deployment_targets.swift_macos release.json)
IPHONEOS_DEPLOYMENT_TARGET=$(jq -r .deployment_targets.swift_ios release.json)
export CARGO_TARGET_DIR="$work/cargo-target-swift"
for target in x86_64-apple-darwin aarch64-apple-darwin x86_64-apple-ios aarch64-apple-ios aarch64-apple-ios-sim; do
    profile=release-smaller
    case "$target" in
        *-darwin) sdk=macosx;;
        aarch64-apple-ios) sdk=iphoneos;;
        aarch64-apple-ios-sim) sdk=iphonesimulator; profile=release;;
        x86_64-apple-ios) sdk=iphonesimulator;;
    esac
    export SDKROOT
    SDKROOT=$(/usr/bin/xcrun --sdk "$sdk" --show-sdk-path)
    export CFLAGS="$base_flags -ffile-prefix-map=$SDKROOT=/apple-sdk"
    (cd "$work/source" && cargo build --frozen --offline --profile "$profile" --no-default-features --features uniffi --target "$target" --lib)
done
cp -R bindings/swift "$work/swift"
mkdir -p "$work/swift/Sources/LDKNode"
chmod -R u+w "$work/swift"
cp "$work/generated/swift/Sources/LDKNode/LDKNode.swift" "$work/swift/Sources/LDKNode/LDKNode.swift"
framework="$work/swift/LDKNodeFFI.xcframework"
version=$(jq -r .bindings_version release.json)
for slice in ios-arm64 ios-arm64_x86_64-simulator macos-arm64_x86_64; do
    sed -i "s/@BINDINGS_VERSION@/$version/g" "$framework/$slice/LDKNodeFFI.framework/Info.plist"
    cp "$work/generated/swift/LDKNodeFFI.h" "$framework/$slice/LDKNodeFFI.framework/Headers/LDKNodeFFI.h"
done
bash scripts/build-swift-metadata.sh release.json "$framework"
cp "$CARGO_TARGET_DIR/aarch64-apple-ios/release-smaller/libldk_node.a" "$framework/ios-arm64/LDKNodeFFI.framework/LDKNodeFFI"
/usr/bin/lipo -create "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/release/libldk_node.a" "$CARGO_TARGET_DIR/x86_64-apple-ios/release-smaller/libldk_node.a" -output "$framework/ios-arm64_x86_64-simulator/LDKNodeFFI.framework/LDKNodeFFI"
/usr/bin/lipo -create "$CARGO_TARGET_DIR/aarch64-apple-darwin/release-smaller/libldk_node.a" "$CARGO_TARGET_DIR/x86_64-apple-darwin/release-smaller/libldk_node.a" -output "$framework/macos-arm64_x86_64/LDKNodeFFI.framework/LDKNodeFFI"
find "$framework" -type d -exec chmod 755 {} +
find "$framework" -type f -exec chmod 644 {} +
find "$framework" -exec touch -d "@$SOURCE_DATE_EPOCH" {} +
(cd "$work/swift" && find LDKNodeFFI.xcframework -type f | sort | zip -X -q "$output/LDKNodeFFI.xcframework.zip" -@)
export SDKROOT
SDKROOT=$(/usr/bin/xcrun --sdk macosx --show-sdk-path)
/usr/bin/xcrun swift build --package-path "$work/swift" --scratch-path "$work/swift-consumer" --disable-sandbox
python -m venv "$work/python-consumer"
"$work/python-consumer/bin/pip" install --no-index "$output"/*macosx_*_arm64.whl
"$work/python-consumer/bin/python" -c 'import ldk_node; ldk_node.default_config()'
