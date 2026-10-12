#!/usr/bin/env bash
# Maintainer-only lock update. Run successful Gradle builds with
# --write-locks --write-verification-metadata sha256 in disposable projects
# first, then provide their GRADLE_USER_HOME directories here. Release builds
# never invoke this script or contact Maven repositories.
set -euo pipefail
(($# >= 2)) || { echo 'Usage: build-lock-maven.sh OUTPUT_JSON GRADLE_HOME...' >&2; exit 1; }
output=$1; shift
scratch=$(mktemp -d /tmp/ldk-maven-lock-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
for cache in "$@"; do
    while IFS= read -r -d '' file; do
        relative=${file#*/files-2.1/}
        IFS=/ read -r group artifact version _checksum name <<< "$relative"
        path="${group//.//}/$artifact/$version/$name"
        # A Gradle variant's cached name can differ from its download URL.
        while IFS= read -r module; do
            alias=$(jq -r --arg name "$name" '[.variants[]?.files[]? | select(.name == $name) | .url] | unique | .[0] // empty' "$module")
            if [[ -n $alias ]]; then
                path=$(realpath -m "/${group//.//}/$artifact/$version/$alias")
                path=${path#/}
                break
            fi
        done < <(find "$cache/caches/modules-2/files-2.1/$group/$artifact/$version" -name '*.module')
        case "$group" in
            com.android*|androidx.*|com.google.testing.platform) repository=https://dl.google.com/dl/android/maven2;;
            *.gradle.plugin|org.jlleitschuh.gradle*) repository=https://plugins.gradle.org/m2;;
            *) repository=https://repo.maven.apache.org/maven2;;
        esac
        url="$repository/$path"
        hash=$(sha256sum "$file"); hash=${hash%% *}
        if ! curl -fsSL --retry 2 "$url" -o "$scratch/download" || ! echo "$hash  $scratch/download" | sha256sum -c - >/dev/null 2>&1; then
            url="https://plugins.gradle.org/m2/$path"
            curl -fsSL --retry 2 "$url" -o "$scratch/download"
            echo "$hash  $scratch/download" | sha256sum -c - >/dev/null || { echo "Cannot locate matching Maven bytes: $relative ($url)" >&2; exit 1; }
        fi
        jq -cn --arg url "$url" --arg path "$path" --arg hash "$hash" '{path:$path,url:$url,sha256:$hash}' >> "$scratch/entries"
    done < <(find "$cache/caches/modules-2/files-2.1" -type f -print0)
done
jq -s 'group_by(.path) | map(if (map(.sha256) | unique | length) != 1 then error("Conflicting Maven bytes for " + .[0].path) else .[0] end) | sort_by(.path)' "$scratch/entries" > "$output"
