# Tari Build Notes

Build options:
- Native compilation on the target platform
- Docker containers with cross-compilation support
- Vagrant/VirtualBox virtualized environments
- QEMU emulation

## Building for Linux x86_64 & ARM64

The Tari project supports building for multiple architectures. You can build natively on your platform or cross-compile for different CPU architectures using our automated scripts.

### Clone the Repository

```bash
mkdir -p ~/src
cd ~/src
git clone git@github.com:tari-project/tari.git
cd tari
```

## Automated Cross-Compilation Setup

The simplest approach is to use the automated cross-compilation scripts, which handle all environment variables and dependency installation.

### For Ubuntu 18.04 through 22.04

Run the unified cross-compilation setup script:

```bash
# Build x86_64 on arm64
export CROSS_DEB_ARCH=amd64
bash ./scripts/cross_compile_ubuntu_18-pre-build.sh x86_64-unknown-linux-gnu

# Build arm64 on x86_64
export CROSS_DEB_ARCH=arm64
bash ./scripts/cross_compile_ubuntu_18-pre-build.sh aarch64-unknown-linux-gnu

# Build riscv64 on x86_64
export CROSS_DEB_ARCH=riscv64
bash ./scripts/cross_compile_ubuntu_18-pre-build.sh riscv64gc-unknown-linux-gnu
```

The script automatically:
- Installs all required development dependencies
- Configures Ubuntu package repositories for the target architecture
- Sets up Rust toolchains and targets
- Configures all necessary environment variables for cross-compilation

### Using Docker

Docker provides a consistent build environment across different systems:

```bash
# Simple Ubuntu container
docker run -it --rm ubuntu:22.04 bash

# macOS with SSH support and port forwarding
docker run -it --rm \
  -v /run/host-services/ssh-auth.sock:/run/host-services/ssh-auth.sock \
  -e SSH_AUTH_SOCK=/run/host-services/ssh-auth.sock \
  -v ${PWD}/../temp/root:/root \
  -v ${PWD}/../tari:/work \
  -w /work \
  -p 0.0.0.0:1230-1240:1230-1240 \
  -u root \
  --platform linux/arm64 \
  ubuntu:22.04 bash
```

## Manual Installation (Advanced)

If you prefer manual setup, follow these steps:

### 1. Install System Dependencies

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
```

### 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
export PATH="$HOME/.cargo/bin:$PATH"
source "$HOME/.cargo/env"
```

### 3. Configure for Cross-Compilation

For ARM64 targets on x86_64:

```bash
rustup target add aarch64-unknown-linux-gnu
rustup toolchain install stable-aarch64-unknown-linux-gnu --force-non-host

# Add cross-compilation tools
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_PATH="/usr/lib/aarch64-linux-gnu/pkgconfig"

# Install cross-compilation packages
sudo dpkg --add-architecture arm64
sudo apt-get update
sudo apt-get install --assume-yes \
  gcc-aarch64-linux-gnu \
  g++-aarch64-linux-gnu \
  libssl-dev:arm64 \
  libudev-dev:arm64 \
  libhidapi-dev:arm64 \
  libdbus-1-dev:arm64
```

For x86_64 targets on ARM64:

```bash
rustup target add x86_64-unknown-linux-gnu
rustup toolchain install stable-x86_64-unknown-linux-gnu --force-non-host

# Add cross-compilation tools
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_PATH="/usr/lib/x86_64-linux-gnu/pkgconfig"

sudo dpkg --add-architecture amd64
sudo apt-get update
sudo apt-get install --assume-yes \
  gcc-x86_64-linux-gnu \
  g++-x86_64-linux-gnu \
  libssl-dev:amd64 \
  libudev-dev:amd64 \
  libhidapi-dev:amd64 \
  libdbus-1-dev:amd64
```

### 4. Verify Rust Setup

```bash
rustup target list --installed
rustup toolchain list
rustup show
```

## Building

### Debug Build

```bash
cargo build \
  --target aarch64-unknown-linux-gnu \
  --bin minotari_miner
```

### Release Build

```bash
cargo build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu
```

### Using Cross

The `cross` tool simplifies cross-compilation by automatically managing environment variables:

```bash
# Single binary
cross build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu

# Multiple binaries
cross build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu \
  --bin minotari_node \
  --bin minotari_console_wallet \
  --bin minotari_merge_mining_proxy \
  --bin minotari_miner

# Build entire workspace
cross build --locked \
  --release --features safe \
  --workspace --exclude tari_integration_tests \
  --target aarch64-unknown-linux-gnu
```

When cross-compiling the entire workspace, use `--workspace --exclude tari_integration_tests` to build all crates except the integration tests, which are designed to run on the native platform.

## Troubleshooting

### Missing Environment Variables

If you see linker errors during compilation, ensure all cross-compilation environment variables are set correctly. The automated `cross_compile_ubuntu_18-pre-build.sh` script handles this automatically.

### Ubuntu 23.04+

For Ubuntu 23.04 and later, the script uses the standard archive repositories (not ports repositories) for all architectures, as multi-architecture support is more mature in recent distributions.
