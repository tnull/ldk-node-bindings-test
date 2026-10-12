#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
exec nix --extra-experimental-features 'nix-command flakes' develop --no-update-lock-file .#build --command bash scripts/build-inner.sh "$@"
