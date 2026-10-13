#!/usr/bin/env bash
# Exercise JAR input layout on both Linux hosts without compiling native code.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
scratch=$(mktemp -d /tmp/ldk-jvm-layout-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/bin" "$scratch/scripts" "$scratch/candidate"
cp -R "$root/bindings" "$scratch/bindings"
cat > "$scratch/bin/uname" <<'EOF'
#!/usr/bin/env bash
if [[ $1 = -s ]]; then echo Linux; else echo "$TEST_ARCH"; fi
EOF
chmod +x "$scratch/bin/uname"
cat > "$scratch/scripts/build-gradle.sh" <<'EOF'
resources="$project/lib/src/main/resources"
cmp "$candidate/jvm-linux-x86_64.so" "$resources/linux-x86-64/libldk_node.so"
cmp "$candidate/jvm-linux-aarch64.so" "$resources/linux-aarch64/libldk_node.so"
cmp "$candidate/jvm-darwin-x86_64.dylib" "$resources/darwin-x86-64/libldk_node.dylib"
cmp "$candidate/jvm-darwin-aarch64.dylib" "$resources/darwin-aarch64/libldk_node.dylib"
EOF
for target in linux-x86_64 linux-aarch64 darwin-x86_64 darwin-aarch64; do
    extension=so
    [[ $target = darwin-* ]] && extension=dylib
    printf '%s\n' "$target" > "$scratch/candidate/jvm-$target.$extension"
done
for arch in aarch64 x86_64; do
    mkdir -p "$scratch/$arch/generated/kotlin/jvm"
    (
        cd "$scratch"
        export PATH="$scratch/bin:$PATH" TEST_ARCH="$arch"
        work="$scratch/$arch" candidate="$scratch/candidate"
        source "$root/scripts/build-jvm.sh"
    )
done
echo 'Both Linux hosts assemble all four JVM native resources.'
