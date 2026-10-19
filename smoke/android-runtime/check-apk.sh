#!/usr/bin/env bash
set -euo pipefail
project=$(cd "$(dirname "$0")" && pwd)
aar="$LDK_ANDROID_CANDIDATE_REPOSITORY/org/lightningdevkit/ldk-node-android/$LDK_ANDROID_CANDIDATE_VERSION/ldk-node-android-$LDK_ANDROID_CANDIDATE_VERSION.aar"
apk="$project/app/build/outputs/apk/release/app-release.apk"
test -s "$project/app/build/outputs/mapping/release/mapping.txt"
grep -q 'org.lightningdevkit.ldknode' "$project/app/build/outputs/mapping/release/mapping.txt"
for abi in x86_64 arm64-v8a armeabi-v7a; do
    expected=$(unzip -p "$aar" "jni/$abi/libldk_node.so" | sha256sum)
    actual=$(unzip -p "$apk" "lib/$abi/libldk_node.so" | sha256sum)
    if [[ $actual != "$expected" ]]; then
        echo "Consumer APK changed the candidate native library: $abi" >&2
        exit 1
    fi
done
test_apk="$project/instrumentation/build/outputs/apk/release/instrumentation-release.apk"
test -s "$test_apk"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
build_tools="$ANDROID_HOME/build-tools/34.0.0"
for module in app instrumentation; do
    jq -e --arg module "$module" '.variantName == "release" and
        (.elements | length == 1) and .elements[0].outputFile == ($module + "-release.apk")' \
        "$project/$module/build/outputs/apk/release/output-metadata.json" > /dev/null
done
"$build_tools/aapt" dump xmltree "$test_apk" AndroidManifest.xml > "$scratch/manifest.txt"
grep -Fq 'android:targetPackage(0x01010021)="org.lightningdevkit.qualification"' "$scratch/manifest.txt"
signer=
for checked_apk in "$apk" "$test_apk"; do
    name=$(basename "$checked_apk" .apk)
    "$build_tools/apksigner" verify --print-certs "$checked_apk" > "$scratch/$name-signature.txt"
    digest=$(sed -n 's/^Signer #1 certificate SHA-256 digest: //p' "$scratch/$name-signature.txt")
    [[ $digest =~ ^[0-9a-f]{64}$ ]]
    if [[ -n $signer && $digest != "$signer" ]]; then
        echo 'Consumer and instrumentation APKs must have the same signer' >&2
        exit 1
    fi
    signer=$digest
    : > "$scratch/$name-classes.txt"
    while IFS= read -r dex; do
        unzip -p "$checked_apk" "$dex" > "$scratch/classes.dex"
        "$build_tools/dexdump" -d "$scratch/classes.dex" > "$scratch/dex.txt"
        awk -F "'" '/^  Class descriptor/ {print $2}' "$scratch/dex.txt" >> "$scratch/$name-classes.txt"
        if [[ $checked_apk = "$apk" ]]; then
            if grep -Eq 'invoke-.*(Ljava/io/File;\.toPath:|Ljava/nio/file/Files;\.createTempDirectory:)' "$scratch/dex.txt"; then
                echo 'Consumer APK must desugar the NIO smoke calls for API 21-23' >&2
                exit 1
            fi
        elif grep -Eq '^  Class descriptor.*(Lj\$/|org/lightningdevkit/ldknode|com/sun/jna|qualification/CandidateSmoke;)' "$scratch/dex.txt"; then
            echo 'Instrumentation APK must not contain bindings, the application test body, or a desugared runtime' >&2
            exit 1
        fi
    done < <(unzip -Z1 "$checked_apk" | grep -E '^classes[0-9]*\.dex$')
    sort -o "$scratch/$name-classes.txt" "$scratch/$name-classes.txt"
done
comm -12 "$scratch/app-release-classes.txt" "$scratch/instrumentation-release-classes.txt" > "$scratch/shared.txt"
if [[ -s $scratch/shared.txt ]]; then
    echo 'Instrumentation APK must not redefine any application classes' >&2
    cat "$scratch/shared.txt" >&2
    exit 1
fi
printf 'Release APKs share a signer and have separate classes; candidate native libraries are unchanged.\n'
