#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 && ${1:-} != --help && ${1:-} != -h ]]; then
    echo "Usage: $0 CANDIDATE_ID CANDIDATE_JSON_SHA256 [--group GROUP] [--output DIR]" >&2
    exit 2
fi
if ! command -v nix >/dev/null 2>&1; then
    echo 'Install Nix with flakes support first; see README.md for host prerequisites.' >&2
    exit 1
fi
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
exec nix --extra-experimental-features 'nix-command flakes' run --no-update-lock-file "$root#release" -- verify-candidate "$@"
