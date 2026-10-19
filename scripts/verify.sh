#!/usr/bin/env bash
set -euo pipefail

report_result() {
    if [[ $1 = 0 ]]; then
        echo 'VERIFICATION SUCCESSFUL'
    else
        echo 'VERIFICATION FAILED' >&2
    fi
    exit "$1"
}
trap 'report_result "$?"' EXIT
for argument; do
    case $argument in -h|--help) trap - EXIT ;; esac
done

if [[ $# -lt 2 && ${1:-} != --help && ${1:-} != -h ]]; then
    echo "Usage: $0 CANDIDATE_ID CANDIDATE_JSON_SHA256 [--repo OWNER/REPO] [--group GROUP] [--output DIR]" >&2
    exit 2
fi
if ! command -v nix >/dev/null 2>&1; then
    echo 'Install Nix with flakes support first; see README.md for host prerequisites.' >&2
    exit 1
fi
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
(
    # Bash otherwise makes asynchronous commands ignore Ctrl-C.
    trap - INT
    exec nix --extra-experimental-features 'nix-command flakes' run --no-update-lock-file "$root#release" -- verify-candidate "$@"
) &
verifier_pid=$!
trap 'kill -INT "$verifier_pid" 2>/dev/null || true; wait "$verifier_pid" 2>/dev/null || true; exit 130' INT
trap 'kill -TERM "$verifier_pid" 2>/dev/null || true; wait "$verifier_pid" 2>/dev/null || true; exit 143' TERM
wait "$verifier_pid"
