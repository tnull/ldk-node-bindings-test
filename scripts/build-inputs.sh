#!/usr/bin/env bash
# Reject manifest edits that would otherwise describe inputs the recipe ignores.
set -euo pipefail
manifest=${1:-release.json}
fail() { echo 'Declared inputs do not match the supported recipe or packaging metadata' >&2; exit 1; }
jq -e '
  .uniffi_version == "0.28.3" and .features == ["uniffi"] and .default_features == false and
  .profiles == {jvm:"release",python:"release-smaller",android:"release-smaller",swift:"release-smaller",swift_arm64_simulator:"release"} and
  .deployment_targets == {python_macos_x86_64:"10.12",python_macos_arm64:"11.0",swift_macos:"12.0",swift_ios:"15.0",android_min_sdk:21,android_target_sdk:31} and
  .archives == {tar_format:"ustar",zip_extra_fields:false,timezone:"UTC"} and
  .tools.gradle == "7.6" and .tools.jdk == "17" and
  .tools.nix == "2.34.8" and .tools.nix_bootstrap == "2.35.2" and
  .tools.android_build_tools == "34.0.0" and .tools.android_compile_sdk == "34" and
  .tools.android_ndk == "27.3.13750724" and
  .tools.xcode == "16.4" and .tools.xcode_build == "16F6"
' "$manifest" >/dev/null || fail
version=$(jq -r .bindings_version "$manifest")
jna=$(jq -r .tools.android_jna "$manifest")
grep -Fq "implementation(\"net.java.dev.jna:jna:$jna@aar\")" bindings/kotlin/ldk-node-android/lib/build.gradle.kts || fail
grep -Fxq "version = \"$version\"" bindings/python/pyproject.toml || fail
for platform in jvm android; do
    grep -Fxq "libraryVersion=$version" "bindings/kotlin/ldk-node-$platform/gradle.properties" || fail
done
if [[ -n ${LDK_NATIVE_SOURCE:-} ]]; then
    native_version=$(jq -r .native.version "$manifest")
    grep -Fxq "version = \"$native_version\"" "$LDK_NATIVE_SOURCE/Cargo.toml" || fail
fi
