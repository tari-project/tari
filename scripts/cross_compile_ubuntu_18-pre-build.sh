#!/usr/bin/env bash
#
# Single script for Ubuntu 18.04 package setup, mostly used for cross-compiling
#

set -e

# APT Proxy for quicker testing
#export HTTP_PROXY_APT=http://apt-proxy.local:3142
if [ ! -z "${HTTP_PROXY_APT}" ] && [ -d "/etc/apt/apt.conf.d/" ]; then
  echo "Setup apt proxy - ${HTTP_PROXY_APT}"
  cat << APT-EoF > /etc/apt/apt.conf.d/proxy.conf
Acquire {
  HTTP::proxy "${HTTP_PROXY_APT}";
  #HTTPS::proxy "http://127.0.0.1:8080";
}
APT-EoF
fi

USAGE="Usage: $0 {target build}
 where target build is one of the following:
  x86_64-unknown-linux-gnu or
  aarch64-unknown-linux-gnu or
  riscv64gc-unknown-linux-gnu
"

if [ "$#" == "0" ]; then
  echo -e "${USAGE}"
  exit 1
fi

if [ -z "${CROSS_DEB_ARCH}" ]; then
  echo -e "Should be run from cross, which sets the env 'CROSS_DEB_ARCH'
  amd64
  arm64
  riscv64
"
  exit 1
fi

DEBIAN_FRONTEND=${DEBIAN_FRONTEND:-noninteractive}
# Hack of Note!
TimeZone=${TimeZone:-"Etc/GMT"}
ln -snf /usr/share/zoneinfo/${TimeZone} /etc/localtime
echo ${TimeZone} > /etc/timezone

targetBuild="${1}"
nativeRunTime=$(uname -m)
echo "Native RunTime is ${nativeRunTime}"

if [ "${nativeRunTime}" == "x86_64" ]; then
  nativeArch=amd64
elif [ "${nativeRunTime}" == "aarch64" ]; then
  nativeArch=arm64
elif [ "${nativeRunTime}" == "riscv64" ]; then
  nativeArch=riscv64
else
  echo "!!Unsupport platform!!"
  exit 1
fi

if [ "${targetBuild}" == "aarch64-unknown-linux-gnu" ]; then
  targetArch=arm64
  targetPlatform=aarch64
elif [ "${targetBuild}" == "x86_64-unknown-linux-gnu" ]; then
  targetArch=amd64
  targetPlatform=x86-64
elif [ "${targetBuild}" == "riscv64gc-unknown-linux-gnu" ]; then
  targetArch=riscv64
  targetPlatform=riscv64
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
  make \
  cmake \
  dh-autoreconf \
  clang \
  g++ \
  libc++-dev \
  libc++abi-dev \
  libprotobuf-dev \
  protobuf-compiler \
  libncurses5-dev \
  libncursesw5-dev \
  libudev-dev \
  libhidapi-dev \
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

  if [[ "${crossArch}" =~ ^(arm|riscv)64$ ]]; then
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

  dpkg --print-architecture
  dpkg --add-architecture ${CROSS_DEB_ARCH}
  dpkg --print-architecture
  apt-get update

  # scripts/install_ubuntu_dependencies-cross_compile.sh x86-64
#    pkg-config-${targetPlatform}-linux-gnu \
  apt-get --assume-yes install \
    gcc-${targetPlatform}-linux-gnu \
    g++-${targetPlatform}-linux-gnu

  # packages needed for Ledger and hidapi
  apt-get --assume-yes install \
    libudev-dev:${CROSS_DEB_ARCH} \
    libhidapi-dev:${CROSS_DEB_ARCH} \
    libssl-dev:${CROSS_DEB_ARCH}

fi

rustup show

rustup target add ${targetBuild}
rustup toolchain install stable-${targetBuild} --force-non-host

rustup target list --installed
rustup toolchain list

rustup show
