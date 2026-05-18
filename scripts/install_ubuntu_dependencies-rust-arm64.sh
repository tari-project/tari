#!/usr/bin/env sh
#
# Install rust target/toolchain for aarch64/arm64
#
export PATH="$HOME/.cargo/bin:$PATH"
rustup target add aarch64-unknown-linux-gnu
rustup toolchain install stable-aarch64-unknown-linux-gnu --force-non-host
