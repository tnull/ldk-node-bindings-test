#!/usr/bin/env bash
# Exercise the actual build entrypoint with a generator that leaves evidence.
set -euo pipefail
scratch=$(mktemp -d /tmp/ldk-bindings-retention-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/bin" "$scratch/verification"
cat > "$scratch/bin/uniffi-bindgen" <<'GENERATOR'
#!/usr/bin/env bash
while (($#)); do
    if [[ $1 = --out-dir ]]; then
        printf 'partial generated source\n' > "$2/evidence.txt"
        printf '%s\n' "$2" > "$LDK_TEST_EVIDENCE_PATH"
        exit 73
    fi
    shift
done
exit 74
GENERATOR
chmod +x "$scratch/bin/uniffi-bindgen"
if PATH="$scratch/bin:$PATH" RELEASE_VERIFY=1 \
    CARGO_TARGET_DIR="$scratch/verification/cargo-target" \
    LDK_TEST_EVIDENCE_PATH="$scratch/evidence-path" \
    bash scripts/build-inner.sh --group generated --output "$scratch/output" > "$scratch/build.log" 2>&1; then
    echo 'Simulated generation failure unexpectedly succeeded' >&2; exit 1
fi
evidence=$(cat "$scratch/evidence-path")
[[ -f "$evidence/evidence.txt" ]] || { echo 'Failed verification deleted generated evidence' >&2; exit 1; }
[[ $evidence = "$scratch/verification/"* ]] || { echo 'Evidence escaped the verifier work directory' >&2; exit 1; }
grep -q 'Failed build workspace retained at' "$scratch/build.log"
grep -q 'partial generated source' "$evidence/evidence.txt"
