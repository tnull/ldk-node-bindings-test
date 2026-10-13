#!/usr/bin/env bash
# A selected host container client must still find its adjacent host helpers.
set -euo pipefail
scratch=$(mktemp -d /tmp/ldk-container-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/bin"
cat > "$scratch/bin/engine" <<'ENGINE'
#!/usr/bin/env bash
[[ $1 = pull ]] || exit 1
ldk-container-helper
ENGINE
cat > "$scratch/bin/ldk-container-helper" <<'HELPER'
#!/usr/bin/env bash
printf 'found host helper\n' > "$LDK_TEST_EVIDENCE_PATH"
exit 73
HELPER
chmod +x "$scratch/bin/"*
set +e
(
    set -euo pipefail
    export CONTAINER_ENGINE="$scratch/bin/engine" LDK_TEST_EVIDENCE_PATH="$scratch/passed"
    group="linux-$(uname -m)"
    source scripts/build-linux.sh
) > "$scratch/build.log" 2>&1
status=$?
set -e
cat "$scratch/build.log"
[[ $status = 73 && -f $scratch/passed ]] || { echo 'Container client cannot find its host helpers' >&2; exit 1; }
