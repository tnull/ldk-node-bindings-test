#!/usr/bin/env bash
# Shrink the actual AAR with its packaged consumer rules, then compare JNA ABI.
set -euo pipefail
aar=$1
jna=$2
mirror=$3
lock=$4
sdk=$5
build_tools=$6
compile_sdk=$7
min_sdk=$8
scratch=$(mktemp -d "${TMPDIR:-/tmp}/ldk-android-consumer-XXXXXXXX")
trap 'rm -rf "$scratch"' EXIT
unzip -p "$aar" classes.jar > "$scratch/classes.jar"
unzip -p "$jna" classes.jar > "$scratch/jna.jar"
# An absent rules file is treated as empty so the regression detects the
# resulting ABI damage, rather than merely checking whether a file exists.
unzip -p "$aar" proguard.txt > "$scratch/consumer.pro" 2>/dev/null || true
printf '\n-keep class org.lightningdevkit.ldknode.Builder { public *; }\n' >> "$scratch/consumer.pro"
libraries=(--lib "$sdk/platforms/android-$compile_sdk/android.jar")
classpath="$scratch/classes.jar:$scratch/jna.jar"
while IFS='=' read -r coordinate configurations; do
    [[ ,$configurations, = *,releaseRuntimeClasspath,* ]] || continue
    IFS=: read -r group artifact version <<< "$coordinate"
    [[ $group:$artifact != net.java.dev.jna:jna ]] || continue
    dependency="$mirror/${group//.//}/$artifact/$version/$artifact-$version"
    if [[ -f $dependency.jar ]]; then
        jar="$dependency.jar"
    elif [[ -f $dependency.aar ]]; then
        jar="$scratch/$group-$artifact.jar"
        unzip -p "$dependency.aar" classes.jar > "$jar"
    else
        grep -q '<packaging>pom</packaging>' "$dependency.pom"
        continue
    fi
    libraries+=(--lib "$jar")
    classpath+=":$jar"
done < "$lock"
r8=(java -cp "$sdk/build-tools/$build_tools/lib/d8.jar" com.android.tools.r8.R8 --release)
# Check Android linking/desugaring, then emit inspectable JVM classes. The
# latter needs the JDK's LambdaMetafactory because classfile output retains it.
"${r8[@]}" --min-api "$min_sdk" "${libraries[@]}" --pg-conf "$scratch/consumer.pro" \
    --output "$scratch/minified.zip" "$scratch/classes.jar" "$scratch/jna.jar"
"${r8[@]}" --classfile --lib "$JAVA_HOME" "${libraries[@]}" --pg-conf "$scratch/consumer.pro" \
    --output "$scratch/minified.jar" "$scratch/classes.jar" "$scratch/jna.jar"
java "$(dirname -- "${BASH_SOURCE[0]}")/checks/AndroidConsumerCheck.java" \
    "$scratch/classes.jar" "$scratch/minified.jar" "$classpath"
