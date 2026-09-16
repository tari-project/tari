#!/usr/bin/env sh
#
# Install rust unattended - https://www.rust-lang.org/tools/install
#
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
export PATH="$HOME/.cargo/bin:$PATH"
. "$HOME/.cargo/env"
