#!/usr/bin/env bash
# Host clients must find their helpers and preserve access to private workdirs.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
scratch=$(mktemp -d /tmp/ldk-container-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/bin" "$scratch/scripts" "$scratch/target/release" "$scratch/output"
cp "$root/release.json" "$scratch/release.json"
printf 'native fixture\n' > "$scratch/target/release/libldk_node.so"
cat > "$scratch/scripts/build-wheel.sh" <<'WHEEL'
# Packaging is unrelated to the container invocation checks.
printf 'wheel fixture\n' > "$4/fixture.whl"
WHEEL
cat > "$scratch/bin/engine" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
ldk-container-helper
case $1 in
    --version) printf '%s version fixture\n' "$LDK_TEST_ENGINE" ;;
    pull) ;;
    run)
        [[ " $* " = *" --network none "* && " $* " = *" --read-only "* ]] || {
            echo 'Container lost its network or filesystem restrictions' >&2; exit 1;
        }
        if [[ " $* " != *" --security-opt label=disable "* ]]; then
            echo 'System mounts need a per-container SELinux label exception' \
                >> "$LDK_TEST_EVIDENCE_PATH.errors"
        fi
        if [[ " $* " = *" -v $LDK_TEST_WORK/output:/wheels:ro "* ]]; then
            printf 'consumer\n' >> "$LDK_TEST_EVIDENCE_PATH"
            exit 0
        fi
        [[ " $* " = *" --user $(id -u):$(id -g) "* &&
            " $* " = *" -v $LDK_TEST_WORK:/work "* ]] || {
            echo 'Container lost its caller identity or private workdir mount' >&2; exit 1;
        }
        keep_id=false
        for argument in "$@"; do
            [[ $argument != --userns=keep-id ]] || keep_id=true
        done
        if [[ $LDK_TEST_ENGINE = podman && $keep_id = false ]]; then
            echo 'Rootless Podman needs keep-id to access the private workdir' >&2; exit 1
        fi
        if [[ $LDK_TEST_ENGINE = docker && $keep_id = true ]]; then
            echo 'Docker does not accept Podman user namespace options' >&2; exit 1
        fi
        printf 'build\n' >> "$LDK_TEST_EVIDENCE_PATH"
        ;;
    *) exit 1 ;;
esac
ENGINE
cat > "$scratch/bin/ldk-container-helper" <<'HELPER'
#!/usr/bin/env bash
exit 0
HELPER
chmod +x "$scratch/bin/"*
for client in docker podman; do
    set +e
    (
        set -euo pipefail
        export CONTAINER_ENGINE="$scratch/bin/engine" LDK_TEST_ENGINE="$client"
        export LDK_TEST_EVIDENCE_PATH="$scratch/$client-passed" LDK_TEST_WORK="$scratch"
        work=$scratch CARGO_TARGET_DIR="$scratch/target" LDK_RUST=$scratch LDK_NATIVE_DEPS=$scratch
        output="$scratch/output"
        group="linux-$(uname -m)"
        cd "$scratch"
        source "$root/scripts/build-linux.sh"
    ) > "$scratch/build.log" 2>&1
    status=$?
    set -e
    cat "$scratch/build.log"
    if [[ -f $scratch/$client-passed.errors ]]; then
        cat "$scratch/$client-passed.errors" >&2
        exit 1
    fi
    [[ $status = 0 && $(cat "$scratch/$client-passed") = $'build\nconsumer' ]] || {
        echo "$client failed its host helper or private workdir checks" >&2; exit 1;
    }
done
