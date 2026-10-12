#!/usr/bin/env bash
# Package a previously built cdylib; no native compilation or pip resolution.
set -euo pipefail
wheel_source=$1
wheel_library=$2
wheel_platform=$3
wheel_output=$4
cp -R bindings/python "$wheel_source"
chmod -R u+w "$wheel_source"
# work is supplied by build-inner.sh via the Linux or Darwin build script.
# shellcheck disable=SC2154
cp "$work/generated/python/ldk_node.py" "$wheel_source/src/ldk_node/ldk_node.py"
cp "$wheel_library" "$wheel_source/src/ldk_node/"
LDK_WHEEL_PLATFORM="$wheel_platform" python -m build --wheel --no-isolation --outdir "$wheel_output" "$wheel_source"
python -m twine check "$wheel_output"/*.whl
