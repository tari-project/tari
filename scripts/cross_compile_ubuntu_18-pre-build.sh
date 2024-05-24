#!/usr/bin/env bash
#
# Single script for Ubuntu 18.04 package setup, mostly used for cross-compiling
#

set -e

USAGE="Usage: $0 target build ie: x86_64-unknown-linux-gnu or aarch64-unknown-linux-gnu"

if [ "$#" == "0" ]; then
  echo "$USAGE"
  exit 1
fi

if [ -z "${CROSS_DEB_ARCH}" ]; then
  echo "Should be run from cross, which sets the env CROSS_DEB_ARCH"
  exit 1
fi

targetBuild="${1}"
nativeRunTime=$(uname -m)
echo "Native RunTime is ${nativeRunTime}"

if [ "${nativeRunTime}" == "x86_64" ]; then
  nativeArch=amd64
  if [ "${targetBuild}" == "aarch64-unknown-linux-gnu" ]; then
    targetArch=arm64
    targetPlatform=aarch64
  else
    targetArch=amd64
    targetPlatform=x86-64
  fi
elif [ "${nativeRunTime}" == "aarch64" ]; then
  nativeArch=arm64
  if [ "${targetBuild}" == "x86_64-unknown-linux-gnu" ]; then
    targetArch=amd64
    targetPlatform=x86-64
  fi
elif [ "${nativeRunTime}" == "riscv64" ]; then
  nativeArch=riscv64
  echo "ToDo!"
else
  echo "!!Unsupport platform!!"
  exit 1
fi

crossArch=${CROSS_DEB_ARCH}
apt-get update

# Base install packages
# scripts/install_ubuntu_dependencies.sh
apt-get install --no-install-recommends --assume-yes \
  apt-transport-https \
  ca-certificates \
  curl \
  gpg \
  bash \
  less \
  openssl \
  libssl-dev \
  pkg-config \
  libsqlite3-dev \
  libsqlite3-0 \
  libreadline-dev \
  git \
  cmake \
  dh-autoreconf \
  clang \
  g++ \
  g++-7 \
  libc++-dev \
  libc++abi-dev \
  libprotobuf-dev \
  protobuf-compiler \
  libncurses5-dev \
  libncursesw5-dev \
  libudev-dev \
  zip

echo "Installing rust ..."
mkdir -p "$HOME/.cargo/bin/"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
export PATH="$HOME/.cargo/bin:$PATH"
. "$HOME/.cargo/env"

# Cross-CPU compile setup
if [ "${CROSS_DEB_ARCH}" != "${nativeArch}" ]; then
  echo "Setup Cross CPU Compile ..."
  sed -i.save -e "s/^deb\ http/deb [arch="${nativeArch}"] http/g" /etc/apt/sources.list

  . /etc/lsb-release
  ubuntu_tag=${DISTRIB_CODENAME}

  if [ "${crossArch}" == "arm64" ]; then
    cat << EoF > /etc/apt/sources.list.d/${ubuntu_tag}-${crossArch}.list
deb [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag} main restricted universe multiverse
# deb-src [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag} main restricted universe multiverse

deb [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag}-updates main restricted universe multiverse
# deb-src [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag}-updates main restricted universe multiverse

deb [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag}-backports main restricted universe multiverse
# deb-src [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag}-backports main restricted universe multiverse

deb [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag}-security main restricted universe multiverse
# deb-src [arch=${crossArch}] http://ports.ubuntu.com/ubuntu-ports ${ubuntu_tag}-security main restricted universe multiverse

deb [arch=${crossArch}] http://archive.canonical.com/ubuntu ${ubuntu_tag} partner
# deb-src [arch=${crossArch}] http://archive.canonical.com/ubuntu ${ubuntu_tag} partner
EoF
  fi

  if [ "${crossArch}" == "amd64" ]; then
    cat << EoF > /etc/apt/sources.list.d/${ubuntu_tag}-${crossArch}.list
deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag} main restricted
# deb-src http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag} main restricted

deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-updates main restricted
# deb-src http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-updates main restricted

deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag} universe
# deb-src http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag} universe
deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-updates universe
# deb-src http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-updates universe

deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag} multiverse
# deb-src http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag} multiverse
deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-updates multiverse
# deb-src http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-updates multiverse

deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-backports main restricted universe multiverse
# deb-src http://archive.ubuntu.com/ubuntu/ ${ubuntu_tag}-backports main restricted universe multiverse

# deb http://archive.canonical.com/ubuntu ${ubuntu_tag} partner
# deb-src http://archive.canonical.com/ubuntu ${ubuntu_tag} partner

deb [arch=amd64] http://security.ubuntu.com/ubuntu/ ${ubuntu_tag}-security main restricted
# deb-src http://security.ubuntu.com/ubuntu/ ${ubuntu_tag}-security main restricted
deb [arch=amd64] http://security.ubuntu.com/ubuntu/ ${ubuntu_tag}-security universe
# deb-src http://security.ubuntu.com/ubuntu/ ${ubuntu_tag}-security universe
deb [arch=amd64] http://security.ubuntu.com/ubuntu/ ${ubuntu_tag}-security multiverse
# deb-src http://security.ubuntu.com/ubuntu/ ${ubuntu_tag}-security multiverse
EoF
  fi

  dpkg --add-architecture ${CROSS_DEB_ARCH}
  apt-get update

  # scripts/install_ubuntu_dependencies-cross_compile.sh x86-64
  apt-get --assume-yes install \
    pkg-config-${targetPlatform}-linux-gnu \
    gcc-${targetPlatform}-linux-gnu \
    g++-${targetPlatform}-linux-gnu

  # packages needed for Ledger and hidapi
  apt-get --assume-yes install \
    libhidapi-dev:${CROSS_DEB_ARCH} \
    libudev-dev:${CROSS_DEB_ARCH}

fi

rustup target add ${targetBuild}
rustup toolchain install stable-${targetBuild} --force-non-host

rustup target list
rustup toolchain list
