#!/usr/bin/env bash
[[ $(uname -s) = Linux && $(uname -m) = x86_64 ]] || { echo 'Android group requires Linux x86_64' >&2; exit 1; }
sdk=$(nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file .#android-sdk --no-link --print-out-paths)
export ANDROID_HOME="$sdk/libexec/android-sdk" ANDROID_SDK_ROOT="$sdk/libexec/android-sdk"
ndk="$ANDROID_HOME/ndk/$(jq -r .tools.android_ndk release.json)/toolchains/llvm/prebuilt/linux-x86_64"
export PATH="$ndk/bin:$PATH" AR="$ndk/bin/llvm-ar"
base_flags=$RUSTFLAGS
min_sdk=$(jq -r .deployment_targets.android_min_sdk release.json)
platform=android
# work is supplied by build-inner.sh, which sources this script.
# shellcheck disable=SC2154
project="$work/android"
cp -R bindings/kotlin/ldk-node-android "$project"
chmod -R u+w "$project"
cp -R "$work/generated/kotlin/android/." "$project/lib/src/main/kotlin/"
for spec in 'x86_64-linux-android x86_64-linux-android x86_64' 'aarch64-linux-android aarch64-linux-android arm64-v8a' 'armv7-linux-androideabi armv7a-linux-androideabi armeabi-v7a'; do
    read -r target clang_target abi <<< "$spec"
    export CC="$ndk/bin/${clang_target}${min_sdk}-clang"
    export CXX="$ndk/bin/${clang_target}${min_sdk}-clang++"
    linker="CARGO_TARGET_${target^^}_LINKER"; linker=${linker//-/_}
    export "$linker=$CC"
    export RUSTFLAGS="$base_flags -C link-args=-Wl,-z,max-page-size=16384,-z,common-page-size=16384"
    (cd "$work/source" && cargo build --frozen --offline --profile release-smaller --no-default-features --features uniffi --target "$target" --lib)
    library="$CARGO_TARGET_DIR/$target/release-smaller/libldk_node.so"
    bash scripts/build-android-native-check.sh "$ndk/bin/llvm-readelf" "$library"
    mkdir -p "$project/lib/src/main/jniLibs/$abi"
    cp "$library" "$project/lib/src/main/jniLibs/$abi/libldk_node.so"
done
source scripts/build-gradle.sh
