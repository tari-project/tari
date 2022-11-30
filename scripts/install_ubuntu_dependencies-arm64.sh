#!/usr/bin/env sh
#
# Install Ubuntu aarch64/arm64 deb dev/tool packages on x86_64
#
apt-get -y install $* \
  pkg-config-aarch64-linux-gnu \
  gcc-aarch64-linux-gnu \
  g++-aarch64-linux-gnu
