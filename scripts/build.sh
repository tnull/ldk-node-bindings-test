#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
# Keep only host selectors and verification state across the Nix boundary.
# In particular, caller Cargo and compiler settings must not alter the recipe.
keep=(--keep HOME)
export LDK_XCODE_DEVELOPER_DIR=${DEVELOPER_DIR:-}
keep+=(--keep LDK_XCODE_DEVELOPER_DIR)
for variable in RELEASE_VERIFY CARGO_TARGET_DIR DOCKER_HOST DOCKER_CONTEXT XDG_RUNTIME_DIR NIX_REMOTE NIX_SSL_CERT_FILE SSL_CERT_FILE; do
    # This entrypoint also runs under macOS's system Bash 3.2, before Nix.
    [[ -z ${!variable:-} ]] || keep+=(--keep "$variable")
done
if command -v "${CONTAINER_ENGINE:-docker}" >/dev/null; then
    CONTAINER_ENGINE=$(command -v "${CONTAINER_ENGINE:-docker}")
    export CONTAINER_ENGINE
    keep+=(--keep CONTAINER_ENGINE)
fi
exec nix --extra-experimental-features 'nix-command flakes' develop --ignore-environment "${keep[@]}" --no-update-lock-file .#build --command bash scripts/build-inner.sh "$@"
