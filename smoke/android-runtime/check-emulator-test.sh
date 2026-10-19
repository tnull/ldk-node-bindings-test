#!/usr/bin/env bash
# Process-boundary controls; these are not Android runtime qualification.
set -euo pipefail
project=$(cd "$(dirname "$0")" && pwd)
script=${1:-$project/emulator.sh}
scratch=$(mktemp -d)
cleanup() {
    while IFS= read -r file; do
        pid=$(cat "$file")
        kill "$pid" 2>/dev/null || true
    done < <(find "$scratch" -name emulator.pid)
    rm -rf "$scratch"
}
trap 'cleanup' EXIT
mkdir -p "$scratch/bin" "$scratch/project" "$scratch/sdk/emulator" \
    "$scratch/sdk/system-images/android-35/google_apis_ps16k/x86_64"
cp "$script" "$scratch/project/emulator.sh"
cp "$project/CheckInstrumentation.java" "$scratch/project/"
printf 'Pkg.Revision=5\n' > "$scratch/sdk/system-images/android-35/google_apis_ps16k/x86_64/source.properties"
cat > "$scratch/bin/tool" <<'TOOL'
#!/usr/bin/env bash
set -euo pipefail
case $(basename "$0") in
sdkmanager) ;;
avdmanager)
    directory=${ANDROID_USER_HOME:+$ANDROID_USER_HOME/avd}
    directory=${directory:-${ANDROID_AVD_HOME:-$CONTROL_ROOT/manager-default}}
    mkdir -p "$directory"
    printf 'path=%s/candidate.avd\n' "$directory" > "$directory/candidate.ini"
    printf '%s\n' "$directory" > "$CONTROL_ROOT/manager-home"
    ;;
emulator)
    case $1 in
    -version|-accel-check) exit 0;;
    -list-avds) test -f "$ANDROID_AVD_HOME/candidate.ini" && echo candidate; exit;;
    esac
    directory=${ANDROID_AVD_HOME:-$CONTROL_ROOT/emulator-default}
    printf '%s\n' "${ANDROID_AVD_HOME:-}" > "$CONTROL_ROOT/emulator-home"
    test -f "$directory/candidate.ini" || { echo 'Unknown AVD name [candidate]' >&2; exit 7; }
    if [[ $CONTROL_MODE = early-exit ]]; then exit 42; fi
    printf '%s\n' "$$" > "$CONTROL_ROOT/emulator.pid"
    exec sleep 120
    ;;
adb)
    if [[ $* = 'shell input keyevent 82' ]]; then
        case $CONTROL_MODE in
        ui-hang) exec sleep 120;;
        ui-rejected) echo 'UI input is unavailable' >&2; exit 9;;
        esac
    fi
    if [[ $CONTROL_MODE = setup-hang && $* = 'shell setprop bionic.linker.16kb.app_compat.enabled false' ]]; then
        touch "$CONTROL_ROOT/setup-command-started"
        exec sleep 120
    fi
    if [[ $CONTROL_MODE = cleanup-hang ]]; then
        case "$*" in
        'logcat -d'|'shell getprop'|'emu kill')
            printf '%s\n' "$*" >> "$CONTROL_ROOT/cleanup-calls"
            exec sleep 120;;
        esac
    fi
    if [[ $1 = shell && -f $CONTROL_ROOT/root-requested && ! -f $CONTROL_ROOT/reconnected ]]; then
        echo 'error: device offline after adbd restart' >&2
        exit 1
    fi
    case "$*" in
    root)
        touch "$CONTROL_ROOT/root-requested" "$CONTROL_ROOT/reconnect-pending"
        echo 'restarting adbd as root';;
    wait-for-device)
        until [[ -f $CONTROL_ROOT/emulator.pid ]] && kill -0 "$(cat "$CONTROL_ROOT/emulator.pid")"; do
            sleep 1
        done;;
    get-state)
        test -f "$CONTROL_ROOT/emulator.pid" || exit 1
        kill -0 "$(cat "$CONTROL_ROOT/emulator.pid")" || exit 1
        if [[ -f $CONTROL_ROOT/reconnect-pending ]]; then
            rm "$CONTROL_ROOT/reconnect-pending"
            echo offline
            exit 1
        fi
        if [[ -f $CONTROL_ROOT/root-requested ]]; then touch "$CONTROL_ROOT/reconnected"; fi
        echo device;;
    'shell id -u')
        if [[ $CONTROL_MODE = root-denied ]]; then uid=2000; else uid=0; fi
        printf '%s\n' "$uid" | tee "$CONTROL_ROOT/observed-uid";;
    'shell getprop sys.boot_completed') echo 1;;
    'shell getprop ro.build.version.sdk')
        if [[ $CONTROL_MODE = cleanup-hang ]]; then echo 99; else echo 35; fi;;
    'shell getconf PAGE_SIZE') echo 16384;;
    'shell setprop '*)
        if [[ $CONTROL_MODE = root-* && ! -f $CONTROL_ROOT/root-requested ]]; then
            echo 'Failed to set property: root required' >&2
            exit 1
        fi
        printf '%s\n' "$4" > "$CONTROL_ROOT/$3";;
    'shell getprop '*) cat "$CONTROL_ROOT/$3";;
    esac
    ;;
gradle)
    touch "$CONTROL_ROOT/gradle-invoked"
    results="$2/instrumentation/build/outputs/androidTest-results/connected"
    mkdir -p "$results"
    printf '%s\n' '<testsuite tests="1" failures="0" errors="0" skipped="0"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"/></testsuite>' > "$results/TEST-control.xml"
    ;;
esac
TOOL
chmod +x "$scratch/bin/tool"
for tool in sdkmanager avdmanager adb gradle; do ln -s tool "$scratch/bin/$tool"; done
ln -s "$scratch/bin/tool" "$scratch/sdk/emulator/emulator"
failed=0
for mode in shared-home early-exit cleanup-hang root-reconnect root-denied ui-hang ui-rejected setup-hang; do
    case_root="$scratch/$mode"
    mkdir -p "$case_root/runner" "$case_root/evidence"
    case $mode in
    cleanup-hang) limit=30;;
    ui-*|setup-hang) limit=12;;
    *) limit=6;;
    esac
    avd_env=()
    if [[ $mode != shared-home ]]; then avd_env=(ANDROID_AVD_HOME="$case_root/existing-avd"); fi
    status=0
    env -u ANDROID_AVD_HOME -u ANDROID_USER_HOME -u ANDROID_EMULATOR_HOME -u ANDROID_SDK_HOME \
        "${avd_env[@]}" \
        PATH="$scratch/bin:$PATH" ANDROID_HOME="$scratch/sdk" \
        RUNNER_TEMP="$case_root/runner" ANDROID_RUNTIME_EVIDENCE="$case_root/evidence" \
        CONTROL_ROOT="$case_root" CONTROL_MODE="$mode" \
        timeout --kill-after=2 "$limit" bash "$scratch/project/emulator.sh" 35 google_apis_ps16k 5 16384 \
        > "$case_root/run.log" 2>&1 || status=$?
    case $mode in
    shared-home)
        if [[ $status != 0 ]] || ! cmp -s "$case_root/manager-home" "$case_root/emulator-home" \
            || [[ ! -s $case_root/manager-home ]] \
            || ! grep -q 'ANDROID RUNTIME QUALIFICATION SUCCESSFUL' "$case_root/run.log"; then
            echo "FAIL: AVD creator and emulator must share an explicit usable home (status $status)"
            failed=1
        elif [[ $(cat "$case_root/manager-home") = "$case_root/evidence"* ]]; then
            echo 'FAIL: AVD data must remain outside uploaded evidence'
            failed=1
        else echo 'PASS: shared AVD home outside uploaded evidence'; fi
        ;;
    early-exit)
        if [[ $status != 1 ]] || ! grep -q 'Emulator exited before connecting' "$case_root/run.log"; then
            echo "FAIL: emulator exit must fail promptly before device connection (status $status)"
            failed=1
        else echo 'PASS: early emulator exit detected promptly'; fi
        ;;
    cleanup-hang)
        if [[ $status != 1 ]] || [[ ! -f $case_root/evidence/device-properties.txt ]] \
            || [[ $(wc -l < "$case_root/cleanup-calls") != 3 ]]; then
            echo "FAIL: hanging diagnostic ADB calls must finish before the outer deadline (status $status)"
            failed=1
        else echo 'PASS: hanging ADB cleanup is bounded'; fi
        ;;
    root-reconnect)
        if [[ $status != 0 ]] || [[ ! -f $case_root/reconnected ]] \
            || [[ $(cat "$case_root/observed-uid" 2>/dev/null) != 0 ]] \
            || [[ $(cat "$case_root/evidence/root-uid.txt" 2>/dev/null) != 0 ]] \
            || [[ $(cat "$case_root/bionic.linker.16kb.app_compat.enabled" 2>/dev/null) != false ]] \
            || [[ $(cat "$case_root/pm.16kb.app_compat.disabled" 2>/dev/null) != true ]] \
            || [[ ! -f $case_root/gradle-invoked ]]; then
            echo "FAIL: compatibility properties require root, reconnect, and UID proof (status $status)"
            failed=1
        else echo 'PASS: root restart reconnects and proves UID before compatibility setup'; fi
        ;;
    root-denied)
        if [[ $status != 1 ]] \
            || ! grep -q 'ADB root required to disable page-size compatibility' "$case_root/run.log" \
            || [[ -f $case_root/bionic.linker.16kb.app_compat.enabled ]] \
            || [[ -f $case_root/gradle-invoked ]]; then
            echo "FAIL: a successful root request must not bypass non-root UID rejection (status $status)"
            failed=1
        else echo 'PASS: non-root UID fails before properties or instrumentation'; fi
        ;;
    ui-hang|ui-rejected)
        if [[ $status != 0 ]] || [[ ! -f $case_root/gradle-invoked ]] \
            || ! grep -q 'ANDROID RUNTIME QUALIFICATION SUCCESSFUL' "$case_root/run.log"; then
            echo "FAIL: headless instrumentation must not depend on UI input ($mode, status $status)"
            failed=1
        else echo "PASS: headless instrumentation ignores unavailable UI input ($mode)"; fi
        ;;
    setup-hang)
        if [[ $status != 124 ]] || [[ ! -f $case_root/setup-command-started ]] \
            || [[ -f $case_root/gradle-invoked ]] \
            || ! grep -Fxq 'ADB command failed (status 124): shell setprop bionic.linker.16kb.app_compat.enabled false' "$case_root/run.log"; then
            echo "FAIL: required ADB setup timeout must identify command/status and stop instrumentation (status $status)"
            failed=1
        else echo 'PASS: required ADB setup timeout is bounded and identifies the failed command'; fi
        ;;
    esac
done
cleanup
trap - EXIT
exit "$failed"
