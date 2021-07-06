#!/usr/bin/env bash

set -e

HASHES_PATH=meta/hashes.txt
SIG_OUTPUT_PATH=meta/hashes.txt.sig

gpg --output $SIG_OUTPUT_PATH --detach-sig $HASHES_PATH
