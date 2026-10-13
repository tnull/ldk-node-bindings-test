#!/usr/bin/env bash
# Exercise the actual build entrypoint with a failed sandboxed generation.
set -euo pipefail
scratch=$(mktemp -d /tmp/ldk-bindings-retention-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/bin" "$scratch/verification"
cat > "$scratch/bin/nix" <<'GENERATOR'
#!/usr/bin/env bash
printf '%s\n' "$(dirname "$CARGO_TARGET_DIR")" > "$LDK_TEST_EVIDENCE_PATH"
echo 'sandboxed generation diagnostic' >&2
exit 73
GENERATOR
chmod +x "$scratch/bin/nix"
if PATH="$scratch/bin:$PATH" RELEASE_VERIFY=1 \
    CARGO_TARGET_DIR="$scratch/verification/cargo-target" \
    LDK_TEST_EVIDENCE_PATH="$scratch/evidence-path" \
    bash scripts/build-inner.sh --group generated --output "$scratch/output" > "$scratch/build.log" 2>&1; then
    echo 'Simulated generation failure unexpectedly succeeded' >&2; exit 1
fi
evidence=$(cat "$scratch/evidence-path")
[[ -f "$evidence/generation.log" && -f "$evidence/source/Cargo.toml" ]] || { echo 'Failed verification deleted build evidence' >&2; exit 1; }
[[ $evidence = "$scratch/verification/"* ]] || { echo 'Evidence escaped the verifier work directory' >&2; exit 1; }
grep -q 'Failed build workspace retained at' "$scratch/build.log"
grep -q 'sandboxed generation diagnostic' "$evidence/generation.log"
