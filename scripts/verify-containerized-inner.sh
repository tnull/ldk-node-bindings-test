#!/usr/bin/env bash
# Container bootstrap; invoked only by verify-containerized.sh.
set -euo pipefail
chmod 1777 /tmp
# These settings live in the disposable container, never on the host.
cat >> /etc/nix/nix.conf <<'NIX'
experimental-features = nix-command flakes
sandbox = false
NIX
mkdir -p /run/ldk-engine
cp /recipe/scripts/verify-containerized-engine.sh /run/ldk-engine/engine
cp /state/engine.env /run/ldk-engine/engine.env
chmod +x /run/ldk-engine/engine
export CONTAINER_ENGINE=/run/ldk-engine/engine
nix build --no-update-lock-file --out-link /run/docker /recipe#container-client
# The host launcher reports the final result after its cleanup finishes.
cd /recipe
exec nix run --no-update-lock-file .#release -- verify-candidate "$@" --output /results
