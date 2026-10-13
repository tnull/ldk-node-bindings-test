#!/usr/bin/env bash
# Validate both XCFramework and individual framework deployment metadata.
set -euo pipefail
manifest=${1:-release.json}
framework=${2:-bindings/swift/LDKNodeFFI.xcframework}
plist_value() { xmllint --nonet --xpath "string($2)" "$1"; }
for slice in ios-arm64 ios-arm64_x86_64-simulator macos-arm64_x86_64; do
    case "$slice" in
        ios-*) key=MinimumOSVersion; target=swift_ios;;
        macos-*) key=LSMinimumSystemVersion; target=swift_macos;;
    esac
    expected=$(jq -r --arg target "$target" '.deployment_targets[$target]' "$manifest")
    case "$slice" in ios-*) platform=iOS;; macos-*) platform=macOS;; esac
    grep -Fq ".$platform(\"$expected\")" "$(dirname "$framework")/Package.swift" || {
        echo "Package.swift deployment target for $platform must be $expected" >&2; exit 1;
    }
    outer="/plist/dict/array/dict[key[.='LibraryIdentifier']/following-sibling::*[1][.='$slice']]"
    for scope in outer slice; do
        if [[ $scope = outer ]]; then
            file="$framework/Info.plist"
            selector="$outer"
        else
            file="$framework/$slice/LDKNodeFFI.framework/Info.plist"
            selector=/plist/dict
        fi
        actual=$(plist_value "$file" "$selector/key[.='$key']/following-sibling::*[1]")
        [[ $actual = "$expected" ]] || {
            echo "$slice $scope $key: expected $expected, found $actual" >&2
            exit 1
        }
    done
done
