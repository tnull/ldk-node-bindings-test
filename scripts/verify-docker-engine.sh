#!/usr/bin/env bash
# Translate the immutable recipe's bind paths for sibling containers.
set -euo pipefail
# This file is generated locally by verify-docker.sh, outside the candidate.
# shellcheck source=/dev/null
source "$(dirname -- "${BASH_SOURCE[0]}")/engine.env"
: "${client:?}" "${store:?}" "${host_work:?}" "${run_id:?}"
arguments=()
if [[ ${1:-} = run ]]; then
    arguments=(run --label "ldk.bindings.verify=$run_id" --userns=host)
    shift
fi
while [[ $# -gt 0 ]]; do
    if [[ $1 = -v ]]; then
        case $2 in
            /nix/store:/nix/store:ro) mount="$store:/nix:ro" ;;
            /tmp/*:*) mount="$host_work/${2#/tmp/}" ;;
            *) echo "Unsupported sibling container mount: $2" >&2; exit 2 ;;
        esac
        arguments+=(-v "$mount")
        shift 2
    else
        arguments+=("$1")
        shift
    fi
done
exec "$client" "${arguments[@]}"
