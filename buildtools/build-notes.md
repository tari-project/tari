# Tari Build Notes

**Note:** These build instructions are for targeting **Linux** as the target platform. The scripts and examples assume Linux binaries as the output.

Build options:
- Native compilation on the target platform
- Docker containers with cross-compilation support
- Vagrant/VirtualBox virtualized environments
- QEMU emulation

## Building for Linux x86_64 & ARM64

The Tari project supports building for multiple architectures. The recommended approach is to use Docker for cross-compilation, as it isolates dependency installation and avoids polluting your local system.

### Clone the Repository

```bash
mkdir -p ~/src
cd ~/src
git clone git@github.com:tari-project/tari.git
cd tari
```

## Recommended: Building with Docker

Docker provides a clean, isolated build environment and automatically invokes the cross-compilation scripts for the target platform. This is the safest approach as it avoids installing numerous system dependencies on your host machine.

### Using Docker with `cross`

First, install `cross`:

```bash
cargo install cross
```

The `cross` tool automatically runs Docker containers and invokes the appropriate `cross_compile_ubuntu_18-pre-build.sh` script for your target platform:

```bash
# Single binary for ARM64
cross build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu \
  --bin minotari_miner

# Multiple binaries for RISC-V
cross build --locked \
  --release --features safe \
  --target riscv64gc-unknown-linux-gnu \
  --bin minotari_node \
  --bin minotari_console_wallet \
  --bin minotari_merge_mining_proxy \
  --bin minotari_miner

# Build entire workspace for x86_64
cross build --locked \
  --release --features safe \
  --workspace --exclude tari_integration_tests \
  --target x86_64-unknown-linux-gnu
```

When cross-compiling the entire workspace, use `--workspace --exclude tari_integration_tests` to build all crates except the integration tests, which are designed to run on the native platform.

### Manual Docker Usage

For more control, you can run Docker containers directly and execute builds inside them:

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

**Docker command options explained:**
- `-it` - Interactive terminal
- `--rm` - Remove container when it exits
- `-v` - Mount volumes (directories from host to container)
- `-e` - Set environment variables
- `-w` - Set working directory in container
- `-p` - Map ports (host:container)
- `-u` - Run as user
- `--platform` - Specify target architecture

Inside the container, you can run the cross-compilation setup script for your target platform:

```bash
# Inside Docker container, set up for target platform
export CROSS_DEB_ARCH=amd64
bash ./scripts/cross_compile_ubuntu_18-pre-build.sh x86_64-unknown-linux-gnu

# Then build
cargo build --locked \
  --release --features safe \
  --target x86_64-unknown-linux-gnu \
  --workspace --exclude tari_integration_tests
```

**Note:** The `cross_compile_ubuntu_18-pre-build.sh` script installs system dependencies and should only be run once per target platform inside a container, not on your host machine.

## Manual Installation (Advanced)

If you want to build on your host machine directly without Docker, you'll need to manually install all dependencies. This approach is not recommended for cross-compilation as it requires significant system-level changes.

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

### 3. Configure for Cross-Compilation (ARM64 on x86_64)

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

### 4. Configure for Cross-Compilation (x86_64 on ARM64)

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

### 5. Verify Rust Setup

```bash
rustup target list --installed
rustup toolchain list
rustup show
```

### 6. Build

```bash
# Single binary
cargo build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu

# Multiple binaries
cargo build --locked \
  --release --features safe \
  --target aarch64-unknown-linux-gnu \
  --bin minotari_node \
  --bin minotari_console_wallet \
  --bin minotari_merge_mining_proxy \
  --bin minotari_miner

# Entire workspace
cargo build --locked \
  --release --features safe \
  --workspace --exclude tari_integration_tests \
  --target aarch64-unknown-linux-gnu
```

**Note:** Cross-compiling on your host machine requires significant system changes and is not recommended. Using Docker is the preferred approach.

## Troubleshooting

### Build Failures with Missing Dependencies

If you encounter linker errors or missing library issues, ensure you're using Docker or have properly set up all cross-compilation dependencies. The Docker approach (using `cross`) automatically handles this for each target platform.

### Understanding the Cross-Compilation Scripts

The `cross_compile_ubuntu_18-pre-build.sh` script is configured in the `Cross.toml` file as a pre-build hook. When using `cross build`, the script is automatically invoked inside a Docker container for your target platform and:

- Installs all required development dependencies for the target architecture
- Configures Ubuntu package repositories for the target platform
- Sets up Rust toolchains and targets
- Configures all necessary environment variables

You should **never run this script directly** on your host machine, as it will modify your system environment and install dependencies you may not want. It's designed to run once per target platform inside an isolated Docker container.

### Ubuntu 23.04+

For Ubuntu 23.04 and later, the script uses the standard archive repositories (not ports repositories) for all architectures, as multi-architecture support is more mature in recent distributions.
