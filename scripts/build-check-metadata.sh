#!/usr/bin/env bash
# Exercise consumer metadata against a changed manifest, without native builds.
set -euo pipefail
mode=${1:-swift}
scratch=$(mktemp -d /tmp/ldk-metadata-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
if [[ $mode = swift ]]; then
    cp -R bindings/swift "$scratch/swift"
    sed -i '/\.iOS(/c\        .iOS("99.0"),' "$scratch/swift/Package.swift"
    if bash scripts/build-swift-metadata.sh release.json "$scratch/swift/LDKNodeFFI.xcframework" > "$scratch/check.log" 2>&1; then
        echo 'Swift Package.swift deployment drift was accepted' >&2; exit 1
    fi
    grep -q 'Package.swift deployment target' "$scratch/check.log"
    exit
fi
cp -R bindings/kotlin/ldk-node-android "$scratch/project"
jq '.deployment_targets.android_min_sdk=23 | .deployment_targets.android_target_sdk=32' release.json > "$scratch/release.json"
cat >> "$scratch/project/lib/build.gradle.kts" <<'GRADLE'

tasks.register("releaseInputsProbe") {
    doLast {
        check(android.defaultConfig.minSdk == 23 && android.defaultConfig.targetSdk == 32) {
            "Gradle deployment targets ignore release.json"
        }
    }
}
GRADLE
gradle=$(nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file .#gradle --no-link --print-out-paths)
sdk=$(nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file .#android-sdk --no-link --print-out-paths)
export LDK_MAVEN_REPOSITORY
LDK_MAVEN_REPOSITORY=$(nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file .#maven-deps --no-link --print-out-paths)
export ANDROID_HOME="$sdk/libexec/android-sdk" ANDROID_SDK_ROOT="$sdk/libexec/android-sdk"
export LDK_RELEASE_MANIFEST="$scratch/release.json" GRADLE_USER_HOME="$scratch/gradle"
printf '127.0.0.1 localhost %s\n' "$(uname -n)" > "$scratch/hosts"
export JAVA_TOOL_OPTIONS="-Djdk.net.hosts.file=$scratch/hosts"
"$gradle/bin/gradle" -p "$scratch/project" --offline --no-daemon --max-workers 2 :lib:releaseInputsProbe
