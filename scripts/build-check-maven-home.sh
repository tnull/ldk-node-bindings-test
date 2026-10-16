#!/usr/bin/env bash
# Probe the real Maven build phase without compiling native libraries.
set -euo pipefail
scratch=$(mktemp -d /tmp/ldk-maven-home-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
export LDK_RECIPE=$PWD
expression='
  let recipe = builtins.getFlake (builtins.getEnv "LDK_RECIPE");
      publication = recipe.lib.mavenPublication {
        system = builtins.currentSystem;
        platform = "jvm";
        source = recipe.outPath;
      };
  in { inherit (publication) buildPhase JAVA_HOME; }
'
nix --extra-experimental-features 'nix-command flakes' eval \
    --impure --json --expr "$expression" > "$scratch/phase.json"
jq -r .buildPhase "$scratch/phase.json" > "$scratch/build-phase.sh"
export JAVA_HOME
JAVA_HOME=$(jq -r .JAVA_HOME "$scratch/phase.json")
(
    export TMPDIR="$scratch/build" HOME="$scratch/missing-home"
    mkdir "$TMPDIR"
    # Android and JVM publication share this setup. Selecting JVM avoids
    # Android's native input checks, while the probe replaces only Gradle.
    # shellcheck disable=SC2317,SC2329 # Called by the evaluated build phase.
    gradle() {
        local java_home directory
        "$JAVA_HOME/bin/java" -XshowSettings:properties -version > "$scratch/java.log" 2>&1
        java_home=$(sed -n 's/^ *user.home = //p' "$scratch/java.log")
        for directory in "$HOME" "$java_home"; do
            [[ $directory = "$TMPDIR/"* && -d $directory && -w $directory ]] || {
                echo "Maven preferences home is not isolated and writable: $directory" >&2
                return 1
            }
            mkdir -p "$directory/.android"
            printf 'preferences are writable\n' > "$directory/.android/test.settings"
        done
        [[ $java_home = "$HOME" ]] || {
            echo 'Java and Android tools disagree on the Maven preferences home' >&2
            return 1
        }
        touch "$scratch/passed"
    }
    # shellcheck disable=SC1091
    source "$scratch/build-phase.sh"
)
test -f "$scratch/passed"
printf 'Maven preferences stay in a writable build-local home.\n'
