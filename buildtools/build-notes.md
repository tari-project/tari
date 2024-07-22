## Build Notes we don't want to lose?

Build options:
 - Native
 - Docker
 - Virtualised
 - Emulated

# Building for Linux x86_64 & ARM64

Using Vagrant and VirtualBox has a baseline for building needs, including tools, libs and testing

Linux ARM64 can be built using Vagrant and VirtualBox or Docker and cross

# Using Docker as a temporary guest for targeting Linux build options
```bash
docker run -it --rm \
  ubuntu:18.04 bash
```

# Testing on OSX (x86_64/arm64) with docker targeting Linux build
Setup a temp folder and tari folder, provide ssh access, with network ports exposed
```bash
docker run -it --rm \
  -v /run/host-services/ssh-auth.sock:/run/host-services/ssh-auth.sock \
  -e SSH_AUTH_SOCK=/run/host-services/ssh-auth.sock \
  -v ${PWD}/../temp/root:/root \
  -v ${PWD}/../tari:/work \
  -w /work \
  -p 0.0.0.0:1230-1240:1230-1240 \
  -u root \
  --platform linux/arm64 \
  ubuntu:18.04 bash
```

# Prep Ubuntu v18.04 for development
# From - https://github.com/tari-project/tari/blob/development/scripts/install_ubuntu_dependencies.sh
```bash
sudo apt-get update
sudo apt-get install --no-install-recommends --assume-yes \
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
  libc++-dev \
  libc++abi-dev \
  libprotobuf-dev \
  protobuf-compiler \
  libncurses5-dev \
  libncursesw5-dev \
  libudev-dev \
  libhidapi-dev \
  zip
```

# Prep Ubuntu for a different CPU architecture than native CPU
an example from arm64 build to x86_64
```bash
export CROSS_DEB_ARCH=amd64
bash ./scripts/cross_compile_ubuntu_18-pre-build.sh x86_64-unknown-linux-gnu
```
an example from x86_64 build to arm64
```bash
export CROSS_DEB_ARCH=arm64
bash ./scripts/cross_compile_ubuntu_18-pre-build.sh aarch64-unknown-linux-gnu
```

# Install rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
# or unattended rust install
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

```bash
export PATH="$HOME/.cargo/bin:$PATH"
source "$HOME/.cargo/env"
```

# Prep rust for cross-compile aarch64/arm64 on x86_64
```bash
rustup target add aarch64-unknown-linux-gnu
rustup toolchain install stable-aarch64-unknown-linux-gnu --force-non-host
```

# Check was tools chains rust has in place
```bash
rustup target list --installed
rustup toolchain list
```

# get/git the code base
```bash
mkdir -p ~/src
cd ~/src
git clone git@github.com:tari-project/tari.git
cd tari
```

# Needed for RandomX
```bash
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export BINDGEN_EXTRA_CLANG_ARGS='--sysroot /usr/aarch64-linux-gnu/include/'
```
Sample of the errors seen, if the above envs are not set
```
Compiling randomx-rs v1.1.13 (https://github.com/tari-project/randomx-rs?tag=v1.1.13#c33f8679)
error: linking with `cc` failed: exit status: 1
|
= note: "cc" "-Wl,--version-script=/tmp/rustcsAUjg7/list" "/tmp/rustcsAUjg7/symbols.o" "/home/vagrant/src/tari/target/aarch64-unknown-linux-gnu/debug/deps/randomx_rs-aa21b69d885376e9.randomx_rs.a9fc037b-cgu.0.rcgu.o"
```
...
```
/usr/bin/ld: /home/vagrant/src/tari/target/aarch64-unknown-linux-gnu/debug/deps/randomx_rs-aa21b69d885376e9.randomx_rs.a9fc037b-cgu.0.rcgu.o: Relocations in generic ELF (EM: 183)
/home/vagrant/src/tari/target/aarch64-unknown-linux-gnu/debug/deps/randomx_rs-aa21b69d885376e9.randomx_rs.a9fc037b-cgu.0.rcgu.o: error adding symbols: File in wrong format
collect2: error: ld returned 1 exit status

error: could not compile `randomx-rs` due to previous error
warning: build failed, waiting for other jobs to finish...
```
Might not need these older envs
```
export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
```

# Needed in the past for croaring-sys
```bash
export ARCH=generic
export ROARING_ARCH=generic
export RUSTFLAGS='-C target_cpu=generic'
export CROSS_COMPILE=true
```

# Build Testing
```bash
cargo build \
  --target aarch64-unknown-linux-gnu \
  --bin minotari_miner
```

# Build target Release
```bash
cargo build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu
```

# Using a single command line build using cross-rs
```bash
cross build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu
```
or
# Building Raspberry Pi Linux compatible binaries on your faster x86_64 or aarch64 computer
```bash
cross build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu \
  --bin minotari_node \
  --bin minotari_console_wallet \
  --bin minotari_merge_mining_proxy \
  --bin minotari_miner
```
