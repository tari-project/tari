#!/usr/bin/env bash
#
# Move all cross-compiling steps into a single sourced script
#

set -e

printenv

if [ "${TARGETARCH}" = "arm64" ] ; then
  platform_env=aarch64
  export BUILD_TARGET="${platform_env}-unknown-linux-gnu/"
  export RUST_TARGET="--target=${platform_env}-unknown-linux-gnu"
  #export ARCH=${ARCH:-generic}
  export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=${platform_env}-linux-gnu-gcc
  export CC_aarch64_unknown_linux_gnu=${platform_env}-linux-gnu-gcc
  export CXX_aarch64_unknown_linux_gnu=${platform_env}-linux-gnu-g++
  export BINDGEN_EXTRA_CLANG_ARGS="--sysroot /usr/${platform_env}-linux-gnu/include/"
  #export RUSTFLAGS="-C target_cpu=$ARCH"
  #export ROARING_ARCH=$ARCH
  rustup target add ${platform_env}-unknown-linux-gnu
  rustup toolchain install stable-${platform_env}-unknown-linux-gnu --force-non-host

  # Check for Debian
  if [ -f "/etc/debian_version" ] ; then
    dpkg --add-architecture ${TARGETARCH}
    apt-get update || true
    apt-get install -y pkg-config libssl-dev:${TARGETARCH} gcc-${platform_env}-linux-gnu g++-${platform_env}-linux-gnu
    export AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_INCLUDE_DIR=/usr/include/${platform_env}-linux-gnu/openssl/
    export PKG_CONFIG_ALLOW_CROSS=1
  fi

elif [ "${TARGETARCH}" = "amd64" ] ; then
  platform_env=x86_64
  platform_env_alt=x86-64
  export BUILD_TARGET="${platform_env}-unknown-linux-gnu/"
  export RUST_TARGET="--target=${platform_env}-unknown-linux-gnu"
  # https://gcc.gnu.org/onlinedocs/gcc/x86-Options.html
  #export ARCH=${ARCH:-x86-64}
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=${platform_env}-linux-gnu-gcc
  export CC_x86_64_unknown_linux_gnu=${platform_env_alt}-linux-gnu-gcc
  export CXX_x86_64_unknown_linux_gnu=${platform_env_alt}-linux-gnu-g++
  export BINDGEN_EXTRA_CLANG_ARGS="--sysroot /usr/${platform_env}-linux-gnu/include/"
  rustup target add ${platform_env}-unknown-linux-gnu
  rustup toolchain install stable-${platform_env}-unknown-linux-gnu --force-non-host

  # Check for Debian
  if [ -f "/etc/debian_version" ] ; then
    dpkg --add-architecture ${TARGETARCH}
    apt-get update
    apt-get install -y pkg-config libssl-dev:${TARGETARCH} gcc-${platform_env_alt}-linux-gnu g++-${platform_env_alt}-linux-gnu
    export X86_64_UNKNOWN_LINUX_GNU_OPENSSL_INCLUDE_DIR=/usr/include/${platform_env}-linux-gnu/openssl/
    export PKG_CONFIG_ALLOW_CROSS=1
  fi

else
  echo "Need to source [ ${0##*/} ] with env TARGETARCH set to either arm64 or amd64"
  exit 1
fi
