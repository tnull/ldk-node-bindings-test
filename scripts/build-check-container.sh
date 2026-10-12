#!/usr/bin/env bash
# Host clients must find their helpers and preserve access to private workdirs.
set -euo pipefail
scratch=$(mktemp -d /tmp/ldk-container-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/bin"
cat > "$scratch/bin/engine" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
ldk-container-helper
case $1 in
    --version) printf '%s version fixture\n' "$LDK_TEST_ENGINE" ;;
    pull) ;;
    run)
        [[ " $* " = *" --user $(id -u):$(id -g) "* &&
            " $* " = *" -v ${LDK_TEST_EVIDENCE_PATH%/*}:/work "* ]] || {
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
        printf 'checked container arguments\n' > "$LDK_TEST_EVIDENCE_PATH"
        exit 73
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
        export LDK_TEST_EVIDENCE_PATH="$scratch/$client-passed"
        work=$scratch CARGO_TARGET_DIR=$scratch LDK_RUST=$scratch LDK_NATIVE_DEPS=$scratch
        group="linux-$(uname -m)"
        source scripts/build-linux.sh
    ) > "$scratch/build.log" 2>&1
    status=$?
    set -e
    cat "$scratch/build.log"
    [[ $status = 73 && -f $scratch/$client-passed ]] || {
        echo "$client failed its host helper or private workdir checks" >&2; exit 1;
    }
done
