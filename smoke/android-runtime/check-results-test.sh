#!/usr/bin/env bash
set -euo pipefail
checker="$(cd "$(dirname "$0")" && pwd)/CheckInstrumentation.java"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/results"
valid='<testsuite tests="1" failures="0" errors="0" skipped="0"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"/></testsuite>'
printf '%s\n' "$valid" > "$scratch/results/TEST-candidate.xml"
java "$checker" "$scratch/results" > "$scratch/check.log" 2>&1 || {
    echo 'The intended successful test was rejected' >&2
    cat "$scratch/check.log" >&2
    exit 1
}

reject() {
    local label=$1 xml=$2
    printf '%s\n' "$xml" > "$scratch/results/TEST-candidate.xml"
    if java "$checker" "$scratch/results" > "$scratch/check.log" 2>&1; then
        echo "Invalid instrumentation result was accepted: $label" >&2
        exit 1
    fi
    grep -q 'Invalid instrumentation evidence:' "$scratch/check.log"
    printf 'Rejected %s\n' "$label"
}
reject 'zero tests' '<testsuite tests="0" failures="0" errors="0" skipped="0"/>'
reject 'skipped test' '<testsuite tests="1" failures="0" errors="0" skipped="1"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"><skipped/></testcase></testsuite>'
reject 'failed test' '<testsuite tests="1" failures="1" errors="0" skipped="0"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"><failure/></testcase></testsuite>'
reject 'test error' '<testsuite tests="1" failures="0" errors="1" skipped="0"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"><error/></testcase></testsuite>'
reject 'wrong test method' '<testsuite tests="1" failures="0" errors="0" skipped="0"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="otherTest"/></testsuite>'
reject 'wrong test class' '<testsuite tests="1" failures="0" errors="0" skipped="0"><testcase classname="other.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"/></testsuite>'
reject 'unreported failure count' '<testsuite tests="1" failures="1"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"/></testsuite>'
reject 'unexpected test count' '<testsuite tests="2" failures="0"><testcase classname="org.lightningdevkit.qualification.CandidateTest" name="minifiedCandidateStartsStopsAndCallsBack"/></testsuite>'
printf '%s\n' "$valid" > "$scratch/results/TEST-duplicate.xml"
reject 'duplicate execution' "$valid"
rm "$scratch/results/TEST-candidate.xml" "$scratch/results/TEST-duplicate.xml"
if java "$checker" "$scratch/results" > "$scratch/check.log" 2>&1; then
    echo 'Missing instrumentation results were accepted' >&2
    exit 1
fi
grep -q 'Invalid instrumentation evidence:' "$scratch/check.log"
echo 'Instrumentation evidence checks passed'
