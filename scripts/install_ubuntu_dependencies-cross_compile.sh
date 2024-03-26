#!/usr/bin/env sh
#
# Install Ubuntu aarch64(arm64)/riscv64 deb dev/tool packages on x86_64
#
USAGE="Usage: $0 ISA_ARCH other packages, ie aarch64"

if [ "$#" == "0" ]; then
    echo "$USAGE"
    exit 1
fi

isa_arch=${1}
shift

apt-get --assume-yes install $* \
  pkg-config-${isa_arch}-linux-gnu \
  gcc-${isa_arch}-linux-gnu \
  g++-${isa_arch}-linux-gnu
