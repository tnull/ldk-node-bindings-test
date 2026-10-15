#!/usr/bin/env bash
# Run within the candidate's clean recipe checkout; credentials stay in the environment.
set -euo pipefail

if ! command -v nix >/dev/null 2>&1; then
    echo 'Install Nix with flakes support first; see README.md for host prerequisites.' >&2
    exit 1
fi
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
exec nix --extra-experimental-features 'nix-command flakes' run --no-update-lock-file "$root#release" -- publish "$@"
