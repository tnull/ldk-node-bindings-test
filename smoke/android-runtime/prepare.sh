#!/usr/bin/env bash
set -euo pipefail
candidate=$1
manifest_sha=$2
output=$3
repo=tnull/ldk-node-bindings-test
recipe=9a3cd7e5185236b3bb9fdd0f22593f47578d5686
[[ $candidate =~ ^candidate-[A-Za-z0-9._-]+$ ]]
[[ $manifest_sha =~ ^[0-9a-f]{64}$ ]]
mkdir -p "$output"
output=$(cd "$output" && pwd)
base="https://github.com/$repo/releases/download/$candidate"
curl --fail --location --proto '=https' --proto-redir '=https' \
    "$base/candidate.json" --output "$output/candidate.json"
printf '%s  %s\n' "$manifest_sha" "$output/candidate.json" | sha256sum --check --strict
jq -e --arg candidate "$candidate" --arg recipe "$recipe" \
    '.schema == 1 and .id == $candidate and .recipe == $recipe and
     .version == "0.7.1" and (.groups.android.files | length) == 5' \
    "$output/candidate.json" > /dev/null
version=$(jq -er .version "$output/candidate.json")
repository="$output/maven"
destination="$repository/org/lightningdevkit/ldk-node-android/$version"
mkdir -p "$destination"
while IFS=$'\t' read -r name digest size; do
    [[ $name =~ ^ldk-node-android-0\.7\.1(-sources|-javadoc)?\.(aar|pom|module|jar)$ ]]
    [[ $digest =~ ^[0-9a-f]{64}$ && $size =~ ^[0-9]+$ ]]
    curl --fail --location --proto '=https' --proto-redir '=https' \
        "$base/$name" --output "$destination/$name"
    printf '%s  %s\n' "$digest" "$destination/$name" | sha256sum --check --strict
    test "$(stat -c %s "$destination/$name")" = "$size"
done < <(jq -er '.groups.android.files[] | [.name, .sha256, .size] | @tsv' "$output/candidate.json")
printf 'LDK_ANDROID_CANDIDATE_REPOSITORY=%s\nLDK_ANDROID_CANDIDATE_VERSION=%s\n' \
    "$repository" "$version" >> "${GITHUB_ENV:?This download entry point runs in GitHub Actions}"
