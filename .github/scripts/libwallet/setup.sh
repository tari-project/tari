#!/bin/bash

sudo apt-get update
sudo apt-get install -y build-essential cmake \
    libsodium-dev libunwind-dev libsystemd-dev liblz4-dev \
    libmicrohttpd-dev \
    clang-format clang-tools clang clangd libc++-dev \
    libc++1 libc++abi-dev libc++abi1 libclang-dev libclang1 \
    liblldb-dev libomp-dev libomp5 lld llvm-dev llvm-runtime llvm \
    valgrind

tar xvzf .github/scripts/libwallet/sqlite-autoconf-3360000.tar.gz
cd sqlite-autoconf-3360000 || exit
./configure
make
sudo make install
cd ..
