#!/usr/bin/env bash
# Copyright 2026 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause
#
# Runs a `cargo test` command once per test name filter and fails if a run fails or passes no test, so a filter
# that stops matching fails instead of passing with "0 passed". Arguments after `--` that start with `-` (such as
# `--exact`) are passed to every run; the others are the filters. Without filters the command runs once.
#
# One `cargo test` can print several "test result" lines (lib, integration and doc tests); each run needs at least
# one that reports a non-zero number passed. Cargo reuses the build, so the extra runs are cheap.
#
# Usage: scripts/cargo_test_ran_tests.sh cargo test --locked -p <crate> --lib -- [--exact] <filter>...
set -euo pipefail

command=()
while [[ $# -gt 0 && "$1" != "--" ]]; do
    command+=("$1")
    shift
done
[[ $# -gt 0 ]] && shift

flags=()
filters=()
for arg in "$@"; do
    if [[ "$arg" == -* ]]; then
        flags+=("$arg")
    else
        filters+=("$arg")
    fi
done

log=$(mktemp)
trap 'rm -f "$log"' EXIT

run() {
    "${command[@]}" -- "$@" 2>&1 | tee "$log"
    if ! grep -Eq "test result: ok\. [1-9][0-9]* passed" "$log"; then
        echo "error: no test passed for: ${command[*]} -- $*" >&2
        exit 1
    fi
}

if [[ ${#filters[@]} -eq 0 ]]; then
    run ${flags[@]+"${flags[@]}"}
else
    for filter in "${filters[@]}"; do
        run ${flags[@]+"${flags[@]}"} "$filter"
    done
fi
