## Build Notes we don't want to loose?

Build options:
 - Native
 - Docker
 - Virtualised
 - Emulated

# Building Linux x86_64 & ARM64

Using Vagrant and VirtualBox has a baseline for building needs, including tools, libs and testing

Linux ARM64 can be built using Vagrant and VirtualBox or Docker and cross

Install rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash
source "$HOME/.cargo/env"
```

From - https://github.com/tari-project/tari/blob/development/scripts/install_ubuntu_dependencies.sh
```bash
sudo apt-get update
sudo apt-get install \
  openssl \
  libssl-dev \
  pkg-config \
  libsqlite3-dev \
  clang-10 \
  git \
  cmake \
  dh-autoreconf \
  libc++-dev \
  libc++abi-dev \
  libprotobuf-dev \
  protobuf-compiler \
  libncurses5-dev \
  libncursesw5-dev \
  zip
```

From Cross.toml
```bash
sudo apt-get install \
  pkg-config-aarch64-linux-gnu \
  gcc-aarch64-linux-gnu \
  g++-aarch64-linux-gnu
```

# Prep rust for cross-compile
```bash
rustup target add aarch64-unknown-linux-gnu
rustup toolchain install stable-aarch64-unknown-linux-gnu
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

# Need for RandomX
```bash
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
export BINDGEN_EXTRA_CLANG_ARGS='--sysroot /usr/aarch64-linux-gnu/include/'
```

# Needed for croaring-sys
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
  --bin tari_miner
```

# Build target Release
```bash
cargo build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu
```

# Using a single command line build using Docker
```bash
cross build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu
```
