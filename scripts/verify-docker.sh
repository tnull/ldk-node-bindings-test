#!/usr/bin/env bash
# Host-only launcher: Nix and all build tools stay in containers.
set -euo pipefail
usage() {
    echo "Usage: $0 CANDIDATE_ID SHA256 [--repo OWNER/REPO] [--group GROUP] [--output DIR]"
    echo 'Uses Podman if installed, otherwise Docker; override with CONTAINER_ENGINE=docker.'
    echo 'Verifies Linux groups only. Requires Git, tar, and a working local container engine.'
}
if [[ ${1:-} = --help || ${1:-} = -h ]]; then usage; exit 0; fi
[[ $# -ge 2 ]] || { usage >&2; exit 2; }
arguments=("$1" "$2")
shift 2
output=
while [[ $# -gt 0 ]]; do
    [[ $# -ge 2 && -n $2 ]] || { usage >&2; exit 2; }
    case $1 in
        --output) output=$2 ;;
        --repo|--group)
            if [[ $1 = --group && $2 = darwin ]]; then
                echo 'Darwin verification requires native macOS and Xcode; use verify.sh.' >&2
                exit 2
            fi
            arguments+=("$1" "$2") ;;
        *) usage >&2; exit 2 ;;
    esac
    shift 2
done
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
engine=${CONTAINER_ENGINE:-}
if [[ -z $engine ]]; then
    if command -v podman >/dev/null 2>&1; then engine=podman; else engine=docker; fi
fi
command -v "$engine" >/dev/null 2>&1 || { echo 'Install Podman or Docker first.' >&2; exit 1; }
engine_command=("$engine")
owner="$(id -u):$(id -g)"
socket_dir=''
service_pid=''
case ${engine##*/} in
    podman)
        [[ $(uname -s) = Linux ]] || { echo 'Use Docker Desktop on macOS; Podman requires a local Linux host.' >&2; exit 1; }
        engine_command+=(--remote=false)
        "${engine_command[@]}" info >/dev/null
        owner=0:0
        ;;
    docker)
        info=$("$engine" info --format '{{.OSType}} {{json .SecurityOptions}}')
        [[ $info = linux\ * ]] || { echo 'The Docker engine must run Linux containers.' >&2; exit 1; }
        [[ $info != *rootless* ]] || owner=0:0
        if [[ -n ${DOCKER_HOST:-} && -z ${DOCKER_CONTEXT:-} ]]; then
            endpoint=$DOCKER_HOST
        else
            endpoint=$("$engine" context inspect --format '{{.Endpoints.docker.Host}}')
        fi
        [[ $endpoint = unix://* ]] || { echo 'Use a local Docker Unix socket; remote engines are unsupported.' >&2; exit 1; }
        socket=${endpoint#unix://}
        [[ -S $socket ]] || { echo "Docker socket is unavailable: $socket" >&2; exit 1; }
        # Docker Desktop exposes its VM's engine here for container bind mounts.
        [[ $(uname -s) != Darwin ]] || socket=/var/run/docker.sock
        ;;
    *) echo 'CONTAINER_ENGINE must select docker or podman.' >&2; exit 2 ;;
esac
if [[ -z $output ]]; then
    mkdir -p "$root/.release"
    state=$(mktemp -d "$root/.release/container-XXXXXXXX")
    output=$state/results
    mkdir "$output"
else
    mkdir -p -- "$output"
    output=$(cd -- "$output" && pwd -P)
    [[ -z $(ls -A -- "$output") ]] || { echo 'Output directory must be empty.' >&2; exit 1; }
    state=$(mktemp -d "$output/../.ldk-verify-XXXXXXXX")
fi
state=$(cd -- "$state" && pwd -P)
# Docker's volume syntax cannot represent colons in host paths.
for path in "$state" "$output"; do
    [[ $path != *:* && $path != *$'\n'* ]] || { echo 'Verification paths must not contain colons or newlines.' >&2; exit 2; }
done
name="ldk-verify-${state##*/}"
image=docker.io/nixos/nix:2.35.2@sha256:7a007c766426c1877758ddc5cb87a965ac131fc78c582ce0083d922d51ae945c
client_pid=''
cleanup() {
    status=$?
    trap - EXIT INT TERM
    "${engine_command[@]}" rm -f "$name" >/dev/null 2>&1 || true
    while read -r sibling; do
        [[ -z $sibling ]] || "${engine_command[@]}" rm -f "$sibling" >/dev/null 2>&1 || true
    done < <("${engine_command[@]}" ps -aq --filter "label=ldk.bindings.verify=$name" || true)
    if [[ -n $client_pid ]]; then
        kill "$client_pid" 2>/dev/null || true
        wait "$client_pid" 2>/dev/null || true
        # Repair ownership even if the verifier was forcibly stopped. Do not
        # follow candidate-created symlinks outside the dedicated directories.
        if ! "${engine_command[@]}" run --rm \
            --userns=host --security-opt label=disable \
            -v "$state:/state" -v "$output:/results" "$image" \
            chown -Rh "$owner" /state /results; then
            echo 'Could not restore ownership of verification results.' >&2
            [[ $status != 0 ]] || status=1
        fi
    fi
    if [[ -n $service_pid ]]; then
        kill "$service_pid" 2>/dev/null || true
        wait "$service_pid" 2>/dev/null || true
    fi
    [[ -z $socket_dir ]] || rm -rf -- "$socket_dir"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ ${engine##*/} = podman ]]; then
    socket_dir=$(mktemp -d /tmp/ldk-verify-socket-XXXXXXXX)
    socket=$socket_dir/engine.sock
    "${engine_command[@]}" system service --time=0 "unix://$socket" > "$state/engine.log" 2>&1 &
    service_pid=$!
    for ((attempt=0; attempt<100; attempt++)); do
        [[ ! -S $socket ]] || break
        kill -0 "$service_pid" 2>/dev/null || break
        sleep 0.1
    done
    [[ -S $socket ]] || { cat "$state/engine.log" >&2; exit 1; }
fi
# Snapshot tracked files, including staged additions, without worktree Git links
# or ignored caches/credentials. No host Nix or container build is needed.
(cd "$root"; git ls-files -z | tar -cf "$state/source.tar" --null -T -)
mkdir "$state/work"
store=${image##*@sha256:}
store="ldk-bindings-nix-${store:0:12}"
printf 'client=/run/docker/bin/docker\nstore=%q\nhost_work=%q\nrun_id=%q\n' "$store" "$state/work" "$name" > "$state/engine.env"
printf 'Verification results: %s\nBuild scratch and logs: %s\nNix cache volume: %s\n' "$output" "$state" "$store"
"${engine_command[@]}" run --rm --name "$name" --userns=host --security-opt label=disable \
    -v "$store:/nix" -v "$state:/state" -v "$state/work:/tmp" \
    -v "$output:/results" -v "$socket:/run/engine.sock" \
    -e DOCKER_HOST=unix:///run/engine.sock \
    "$image" bash -euc 'mkdir /recipe; tar -xf /state/source.tar -C /recipe; exec bash /recipe/scripts/verify-docker-inner.sh "$@"' \
    verify-docker "${arguments[@]}" &
client_pid=$!
wait "$client_pid"
