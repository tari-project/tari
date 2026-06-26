<!-- CI / Build Status -->
[![CI](../../actions/workflows/ci.yml/badge.svg?branch=development)](../../actions/workflows/ci.yml)
[![Integration Tests](../../actions/workflows/integration_tests.yml/badge.svg?branch=development)](../../actions/workflows/integration_tests.yml)
[![Docker Build](../../actions/workflows/build_dockers.yml/badge.svg?branch=development)](../../actions/workflows/build_dockers.yml)
[![Binary Build](../../actions/workflows/build_binaries.yml/badge.svg?branch=development)](../../actions/workflows/build_binaries.yml)

<!-- Release & License -->
[![Release](https://img.shields.io/github/v/release/tari-project/tari?sort=semver)](https://github.com/tari-project/tari/releases)
[![License](https://img.shields.io/github/license/tari-project/tari)](https://github.com/tari-project/tari/blob/development/LICENSE)

[![Coverage Status](https://coveralls.io/repos/github/tari-project/tari/badge.svg?branch=development)](https://coveralls.io/github/tari-project/tari?branch=development)

# The Tari protocol

A number of applications have been developed by the Tari community to implement the Tari protocol. These are:

- Minotari Base Node
- Minotari Wallet
- Minotari Miner
- Minotari Merge Mining Proxy
- Minotari MCP Servers (for AI integration)
- Minotari Aurora wallets for Android and iOS

The core applications and MCP servers are documented in this README (see [wallet-android](https://github.com/tari-project/wallet-android) and [wallet-ios](https://github.com/tari-project/wallet-ios) for mobile wallets' repos).

## Developers
Want to contribute? Start by reading the [Contributing Guide](Contributing.md) and the [Reviewing Guide](docs/src/reviewing_guide.md).
Want to discuss new ideas? Head over to the [Tari Forum](https://community.tari.com/) and join the conversation.
Want to learn about the technical details of Tari? Head over to the [Tari RFCs](https://rfc.tari.com/).

## Installing using binaries

### Versions
The recommended version is available at: [Latest version](https://github.com/tari-project/tari/releases/latest)

### Running tests
Tests can be run using Nextest. Install it with the following command:

```bash
cargo install cargo-nextest
```

Then run the tests with:

```bash
cargo ci-test
```

### Download

[Download binaries](https://tari.com/downloads/) from [tari.com](https://www.tari.com/). This is the easiest way to run a Tari node, but you're
essentially trusting the person that built and uploaded them that nothing untoward has happened.

Hashes of the binaries are available alongside the downloads.
You can get the hash of your download by opening a terminal or command prompt and running the following:

(\*nix)

    shasum -a256 <PATH_TO_BINARY_INSTALL_FILE>

(Windows)

    certUtil -hashfile <PATH_TO_BINARY_INSTALL_FILE> SHA256

If the result doesn't match the published hash, don't run the binary.
Note that this only checks that your binary was downloaded correctly; it cannot detect if the binary was replaced by a bad actor.
If you need to ensure that your binary matches the source, see [Building from source](#building-from-source) below.


### Runtime links

#### Start all applications individually

  **Note**: The Tor console will output `[notice] Bootstrapped 100% (done): Done`
  when the Tor services have fully started.

- Depending on your choice of mining:

  - SHA3 standalone mining
    - Execute the `start_minotari_miner` soft link/shortcut.
  - Merge mining with Monero
    - Execute the `start_minotari_merge_mining_proxy` soft link/shortcut.
    - Execute the `start_xmrig` shortcut.

## Building from source

To build the Minotari codebase from source, there are a few dependencies you need to have installed.

### Network versions
It's important to note that when compiling from source, you need to specify the target network at compile time using an environment variable. For example, to compile for the Esmeralda testnet, you would run:

```
    TARI_TARGET_NETWORK=testnet cargo build --release
```

| TARI_TARGET_NETWORK | Available Networks |
| --- | --- |
| "" | esmeralda, igor, localnet |
| testnet | esmeralda, igor, localnet |
| nextnet | nextnet |
| mainnet | mainnet, stagenet |


### Install development packages

First you'll need to make sure you have a full development environment set up:

#### (macOS)

```
brew update
brew install coreutils tor openssl \
  cmake make libtool autoconf automake protobuf
```

#### (Ubuntu 18.04, including WSL-2 on Windows)

```
sudo apt-get update
sudo apt-get -y install openssl libssl-dev pkg-config libsqlite3-dev clang git cmake libc++-dev libc++abi-dev libprotobuf-dev protobuf-compiler libncurses5-dev libncursesw5-dev
sudo apt-get install -y wget apt-transport-https
sudo wget -q "https://packages.microsoft.com/config/ubuntu/$(lsb_release -rs)/packages-microsoft-prod.deb"
sudo dpkg -i packages-microsoft-prod.deb
sudo apt-get update
sudo add-apt-repository universe
sudo apt-get install -y powershell
```

#### (Windows)

Please follow the guide [located here](https://github.com/tari-project/tari/blob/development/buildtools/windows-dev-environment-notes.md) for setting up your build environment on Windows.

### Build

Grab a cup of coffee and begin the Tari build.

(\*nix)

    cd tari
    cargo build --release

(Windows)

This is similar to building in Ubuntu, except the Microsoft Visual Studio environment must be sourced. Open the
appropriate _x64\x86 Native Tools Command Prompt for VS 2019_, and in your main Tari directory perform the
build, which will create the executable inside your `%USERPROFILE%\Code\tari\target\release` directory:

    cd %USERPROFILE%\Code\tari
    cargo build --release

A successful build should output something like this:

```
   Compiling minotari_wallet v0.0.9 (.../tari/base_layer/wallet)
   Compiling minotari_wallet_ffi v0.0.9 (.../tari/base_layer/wallet_ffi)
   Compiling minotari_node v0.0.9 (.../tari/applications/minotari_node)
    Finished release [optimized] target(s) in 12m 24s
```

Compiled executables can be found at these paths:

    ./target/release/minotari_node
    ./target/release/minotari_console_wallet
    ./target/release/minotari_merge_mining_proxy
    ./target/release/minotari_miner
    ./target/release/minotari_mcp_wallet
    ./target/release/minotari_mcp_node

Alternatively, `cargo` can build and install the executable into `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on Windows), so it will be executable from anywhere
on your system:

    cargo install --path=applications/minotari_node --force
    cargo install --path=applications/minotari_console_wallet --force
    cargo install --path=applications/minotari_merge_mining_proxy --force
    cargo install --path=applications/minotari_miner --force
    cargo install --path=applications/minotari_mcp_wallet --force
    cargo install --path=applications/minotari_mcp_node --force

### Run

The executables will either be inside your `~/tari/target/release` (on Linux) or `%USERPROFILE%\Code\tari\target\release`
(on Windows) directory, or alternatively, inside your `~/.cargo/bin` (on Linux) or `%USERPROFILE%\.cargo\bin` (on Windows)
directory, depending on the build method used above. If you used the former method, you can run the executables
from that directory or copy them somewhere more convenient.

On Windows, make sure to start the Tor service via `%USERPROFILE%\Code\tari\applications\minotari_node\windows\start_tor.lnk`.
On Linux and macOS, Tor is included in the binary.

Running:

    minotari_node

    minotari_console_wallet

    minotari_merge_mining_proxy

    minotari_miner

    minotari_mcp_wallet

    minotari_mcp_node

Alternatively, you can run the Tari applications from your source directory using `cargo`. Omit the `--release`
flag to run in debug mode:

    cargo run --bin minotari_node --release

    cargo run --bin minotari_merge_mining_proxy --release

    cargo run --bin minotari_console_wallet --release

    cargo run --bin minotari_miner --release

    cargo run --bin minotari_mcp_wallet --release

    cargo run --bin minotari_mcp_node --release

With the default options, the blockchain database, wallet database, console wallet database, log files, and all
configuration files will be created in `~/.tari` (on Linux) or `%USERPROFILE%\.tari` (on Windows).
You can override this by specifying `--base-path <base-path>` on the command line.

## Advanced build configurations

- Vagrant: See [Building with Vagrant](https://github.com/tari-project/tari/issues/1407) for using Vagrant to build and run a base node in a clean environment.

## Using Docker

### Running the base node with a Docker image

Minotari Base Node Docker images can be found at https://quay.io/repository/tarilabs/minotari_node

Using `docker-compose.yaml`:

```
version: "3"

services:
  minotari_node:
    image: quay.io/tarilabs/minotari_node:latest-nextnet
    restart: unless-stopped
    volumes:
      - ./data:/var/tari
# These 2 params are required for an interactive docker-compose session
    stdin_open: true
    tty: true
    expose:
      - 18142
    ports:
      - "18142:18142"
```

The Docker image runs with `/var/tari` as the Tari home directory. Mounting the host `./data` directory there persists
the node base path (`/var/tari/node`) and config path (`/var/tari/config/config.toml`).

Then run `docker-compose up -d` to start your docker service.

Check the running state with `docker-compose ps`:

```
        Name           Command    State            Ports
------------------------------------------------------------------
tbn_minotari_node_1   start.sh   Up      0.0.0.0:18142->18142/tcp
```

To connect to the console, use `docker ps` to get the container ID of the `minotari_node` container:

```
CONTAINER ID        IMAGE                                    COMMAND             CREATED             STATUS              PORTS                      NAMES
73427509a4bb        quay.io/tarilabs/minotari_node:v0.5.4   "start.sh"          45 minutes ago      Up 26 minutes       0.0.0.0:18142->18142/tcp   tbn_minotari_node_1
```

With the container ID `73427509a4bb`, connect to the `minotari_node` console using `docker attach 73427509a4bb`:

```
>> help
Available commands are:
help, version, get-chain-metadata, list-peers, reset-offline-peers, ban-peer, unban-peer, list-connections, list-headers,
check-db, calc-timing, discover-peer, get-block, search-utxo, search-kernel, search-stxo, get-mempool-stats,
get-mempool-state, whoami, get-state-info, quit, exit
>> get-chain-metadata
Height of longest chain : 5228
Geometric mean of longest chain : 5892870
Best block : 2c4f92854b2160324b8afebaa476b39be4004d2a7a19c69dd2d4e4da257bfee2
Pruning horizon : 0
Effective pruned height : 0
>> get-state-info
Current state machine state:
Synchronizing blocks: Syncing from the following peers:
510c83279adc7cb7d7dda0aa07
Syncing 5229/5233
```

---

## AI Integration (MCP Servers)

Tari provides Model Context Protocol (MCP) servers that enable AI agents like Claude to interact securely with Tari blockchain functionality.

### MCP Applications

- **`minotari_mcp_wallet`**: Provides secure wallet operations for AI agents
- **`minotari_mcp_node`**: Enables AI agents to query blockchain and node information
- **`minotari_mcp_common`**: Shared infrastructure for building secure MCP servers

### Quick Start

1. **Start your Tari wallet with gRPC enabled**:
   ```bash
   minotari_console_wallet --enable-grpc
   ```

2. **Start the MCP wallet server** (read-only, safe for AI):
   ```bash
   minotari_mcp_wallet --mcp-enabled
   ```

3. **Start the MCP node server**:
   ```bash
   minotari_mcp_node --mcp-enabled
   ```

### Security Features

- **Local-only binding**: Servers only bind to 127.0.0.1 for security
- **Permission levels**: Separate read-only and control operations
- **Rate limiting**: Configurable request limits per client
- **Audit logging**: Comprehensive operation tracking
- **User confirmation**: Optional confirmation for value transfers

For complete documentation, see:
- [MCP Implementation Guide](docs/mcp/TARI_MCP_IMPLEMENTATION.md)
- [Wallet MCP Server](applications/minotari_mcp_wallet/README.md)
- [Common MCP Framework](applications/minotari_mcp_common/README.md)

For more information about the Model Context Protocol, visit [modelcontextprotocol.io](https://modelcontextprotocol.io/specification/2025-03-26).

# Project documentation

- [RFC documents](https://rfc.tari.com) are hosted on Github Pages. The source markdown is in the `RFC` directory.
- Source code documentation is hosted on [docs.rs](https://docs.rs)
- [RFC repo](https://github.com/tari-project/rfcs)


### Source code documentation

Run

    cargo doc

to generate the documentation. The generated HTML sits in `target/doc/`. Alternatively, to open a specific package's documentation directly in your browser, run:

    cargo doc -p <package> --open
