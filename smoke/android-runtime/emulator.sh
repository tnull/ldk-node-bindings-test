#!/usr/bin/env bash
set -euo pipefail
api=$1
tag=$2
revision=$3
page_size=$4
project=$(cd "$(dirname "$0")" && pwd)
evidence=${ANDROID_RUNTIME_EVIDENCE:?}
mkdir -p "$evidence"
sdk=${ANDROID_HOME:?}
image="system-images;android-$api;$tag;x86_64"
[[ $api =~ ^(21|22|23|35)$ && $tag =~ ^google_apis(_ps16k)?$ ]]
sdkmanager --install emulator platform-tools 'platforms;android-34' 'build-tools;34.0.0' "$image"
properties="$sdk/system-images/android-$api/$tag/x86_64/source.properties"
grep -Eq "^Pkg\.Revision[[:space:]]*=[[:space:]]*${revision}[[:space:]]*$" "$properties"
cp "$properties" "$evidence/system-image.properties"
"$sdk/emulator/emulator" -version > "$evidence/emulator-version.txt"
"$sdk/emulator/emulator" -accel-check
avd_root=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/android-avd-XXXXXXXX")
export ANDROID_SDK_HOME="$avd_root" ANDROID_USER_HOME="$avd_root/.android"
export ANDROID_EMULATOR_HOME="$ANDROID_USER_HOME" ANDROID_AVD_HOME="$ANDROID_USER_HOME/avd"
mkdir -p "$ANDROID_USER_HOME" "$ANDROID_AVD_HOME"
emulator_pid=
run_adb() {
    local status=0
    timeout --kill-after=2 5 adb "$@" || status=$?
    if ((status != 0)); then
        printf 'ADB command failed (status %s):' "$status" >&2
        printf ' %q' "$@" >&2
        printf '\n' >&2
    fi
    return "$status"
}
wait_for_device() {
    local deadline=$((SECONDS + $1))
    while true; do
        if ! kill -0 "$emulator_pid" 2>/dev/null; then
            local status=0
            wait "$emulator_pid" || status=$?
            echo "Emulator exited before connecting (status $status)" >&2
            cat "$evidence/emulator.log" >&2
            exit 1
        fi
        if [[ $(run_adb get-state 2>/dev/null || true) = device ]]; then return; fi
        ((SECONDS < deadline)) || { echo 'Emulator did not connect to ADB' >&2; exit 1; }
        sleep 1
    done
}
cleanup() {
    if [[ -n $emulator_pid ]]; then
        run_adb logcat -d > "$evidence/logcat.txt" 2>&1 || true
        run_adb shell getprop > "$evidence/device-properties.txt" 2>&1 || true
        run_adb emu kill > /dev/null 2>&1 || true
        kill -KILL "$emulator_pid" 2>/dev/null || true
    fi
    rm -rf "$avd_root"
}
trap cleanup EXIT
printf 'no\n' | avdmanager create avd --force --name candidate --package "$image" \
    --device pixel --path "$ANDROID_AVD_HOME/candidate.avd"
test -f "$ANDROID_AVD_HOME/candidate.ini"
"$sdk/emulator/emulator" -list-avds | grep -Fx candidate
cp "$ANDROID_AVD_HOME/candidate.ini" "$evidence/avd.ini"
"$sdk/emulator/emulator" -avd candidate -no-window -no-audio -no-boot-anim \
    -no-snapshot -wipe-data -gpu swiftshader_indirect -memory 3072 \
    > "$evidence/emulator.log" 2>&1 &
emulator_pid=$!
wait_for_device 600
deadline=$((SECONDS + 600))
until [[ $(run_adb shell getprop sys.boot_completed | tr -d '\r') = 1 ]]; do
    kill -0 "$emulator_pid"
    ((SECONDS < deadline)) || { echo 'Emulator did not finish booting' >&2; exit 1; }
    sleep 5
done
test "$(run_adb shell getprop ro.build.version.sdk | tr -d '\r')" = "$api"
if [[ $page_size = 16384 ]]; then
    actual_page_size=$(run_adb shell getconf PAGE_SIZE | tr -d '\r')
else
    # Android 5's toolbox predates getconf. The app also checks sysconf directly.
    actual_page_size=$(run_adb shell cat /proc/self/smaps | awk '$1 == "KernelPageSize:" && !size {size=$2*1024} END {print size}')
fi
printf '%s\n' "$actual_page_size" > "$evidence/page-size.txt"
test "$actual_page_size" = "$page_size"
if [[ $page_size = 16384 ]]; then
    run_adb root | tee "$evidence/adb-root.txt"
    wait_for_device 60
    uid=$(run_adb shell id -u | tr -d '\r')
    printf '%s\n' "$uid" > "$evidence/root-uid.txt"
    [[ $uid = 0 ]] || { echo 'ADB root required to disable page-size compatibility' >&2; exit 1; }
    run_adb shell setprop bionic.linker.16kb.app_compat.enabled false
    run_adb shell setprop pm.16kb.app_compat.disabled true
    test "$(run_adb shell getprop bionic.linker.16kb.app_compat.enabled | tr -d '\r')" = false
    test "$(run_adb shell getprop pm.16kb.app_compat.disabled | tr -d '\r')" = true
fi
results="$project/instrumentation/build/outputs/androidTest-results/connected"
rm -rf "$results"
gradle -p "$project" --no-daemon --no-build-cache --info --stacktrace :instrumentation:connectedReleaseAndroidTest \
    2>&1 | tee "$evidence/connected-test.log"
java "$project/CheckInstrumentation.java" "$results" | tee "$evidence/testcase-verification.txt"
printf 'ANDROID RUNTIME QUALIFICATION SUCCESSFUL: API %s, page size %s\n' "$api" "$page_size"
