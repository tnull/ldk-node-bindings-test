#!/usr/bin/env bash
# The populated project (generated sources and native inputs) is copied into a
# sandboxed Nix derivation. Fresh Gradle caches can resolve only pinned files.
# project and platform are supplied by build-android.sh or build-jvm.sh.
# shellcheck disable=SC2154
export LDK_GRADLE_PROJECT="$project" LDK_GRADLE_PLATFORM="$platform" LDK_RECIPE="$PWD"
expression='
  let recipe = builtins.getFlake (builtins.getEnv "LDK_RECIPE");
  in recipe.lib.mavenPublication {
    system = builtins.currentSystem;
    platform = builtins.getEnv "LDK_GRADLE_PLATFORM";
    source = builtins.path { path = builtins.getEnv "LDK_GRADLE_PROJECT"; name = "ldk-gradle-inputs"; };
  }
'
nix --extra-experimental-features 'nix-command flakes' build --impure --no-update-lock-file --no-link --expr "$expression"
publication=$(nix --extra-experimental-features 'nix-command flakes' build --impure --no-update-lock-file --no-link --print-out-paths --rebuild --expr "$expression")
# output and group are supplied by build-inner.sh.
# shellcheck disable=SC2154
while IFS= read -r file; do cp "$publication/$file" "$output/$file"; done < <(jq -r --arg group "$group" '.groups[$group][]' release.json)
