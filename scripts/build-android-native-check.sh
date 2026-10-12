#!/usr/bin/env bash
# Every shipped native dependency must support Android's 16 KiB pages.
set -euo pipefail
readelf=$1; shift
for library in "$@"; do
    alignments=$("$readelf" --program-headers --wide "$library" | awk '$1 == "LOAD" { print $NF }')
    [[ -n $alignments ]] || { echo "No LOAD segments in $library" >&2; exit 1; }
    for alignment in $alignments; do
        if [[ ! $alignment =~ ^0x[0-9a-fA-F]+$ ]] || ((alignment < 16384)); then
            echo "Android library requires 16 KiB LOAD alignment: $library ($alignment)" >&2
            exit 1
        fi
    done
    if "$readelf" --dynamic "$library" | grep /nix/store; then
        echo "Nix runtime dependency in Android library: $library" >&2; exit 1
    fi
done
