#!/usr/bin/env bash
# Copyright 2026 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause
#
# Runs the given `cargo test ...` command and fails if it fails, or if none of its test binaries passed a test. A test
# name filter that stops matching would otherwise pass with "0 passed". One `cargo test` can print several
# "test result" lines (lib, integration and doc tests); at least one must report a non-zero number passed.
#
# Usage: scripts/cargo_test_ran_tests.sh cargo test --locked -p <crate> --lib -- <filter>
set -euo pipefail

log=$(mktemp)
trap 'rm -f "$log"' EXIT

"$@" 2>&1 | tee "$log"

if ! grep -Eq "test result: ok\. [1-9][0-9]* passed" "$log"; then
    echo "error: no test passed; check that the test name filters still match" >&2
    exit 1
fi
