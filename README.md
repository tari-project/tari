<!-- CI / Build Status -->
[![CI](https://github.com/tari-project/tari/actions/workflows/ci.yml/badge.svg)](https://github.com/tari-project/tari/actions/workflows/ci.yml)
[![Integration Tests](https://github.com/tari-project/tari/actions/workflows/integration_tests.yml/badge.svg)](https://github.com/tari-project/tari/actions/workflows/integration_tests.yml)
[![Docker Build](https://github.com/tari-project/tari/actions/workflows/build_dockers.yml/badge.svg)](https://github.com/tari-project/tari/actions/workflows/build_dockers.yml)
[![Binary Build](https://github.com/tari-project/tari/actions/workflows/build_binaries.yml/badge.svg)](https://github.com/tari-project/tari/actions/workflows/build_binaries.yml)

<!-- Release & License -->
[![Release](https://img.shields.io/github/v/release/tari-project/tari?sort=semver)](https://github.com/tari-project/tari/releases)
[![License](https://img.shields.io/github/license/tari-project/tari)](https://github.com/tari-project/tari/blob/development/LICENSE)
[![Coverage Status](https://coveralls.io/repos/github/tari-project/tari/badge.svg?branch=development)](https://coveralls.io/github/tari-project/tari?branch=development)

# The Tari protocol

This repository contains the Rust implementation of the Tari base layer protocol — the blockchain, its consensus rules,
the peer-to-peer network stack, and the applications that run on top of them.

## Applications

| Application | Binary | What it does |
| --- | --- | --- |
| Base Node | `minotari_node` | Full node: stores and validates the blockchain, serves the P2P network and a gRPC API. See [README](applications/minotari_node/README.md). |
| Console Wallet | `minotari_console_wallet` | Terminal wallet for sending, receiving and managing funds. See [README](applications/minotari_console_wallet/README.md). |
| Miner | `minotari_miner` | Standalone SHA-3 miner. See [README](applications/minotari_miner/README.md). |
| Merge Mining Proxy | `minotari_merge_mining_proxy` | Lets you merge mine Tari alongside Monero via XMRig. |
| Offline Signer | `minotari_offline_signer` | Signs one-sided transactions on an air-gapped machine. See [README](applications/minotari_offline_signer/README.md). |

These five are the binaries published with each [release](https://github.com/tari-project/tari/releases).

The repository also contains supporting binaries that are built from source but not shipped in releases:

| Application | Binary | What it does |
| --- | --- | --- |
| Peer Sync | `minotari_peer_sync` | Diagnostic tool that runs only the base node's peer discovery. See [README](applications/minotari_peer_sync/README.md). |
| Utils | `minotari_utils` | Assorted base node operational utilities. See [README](applications/minotari_utils/README.md). |
| Ledger Wallet | — | Ledger hardware wallet app, used by the console wallet's `ledger` feature. |
| MCP Servers | `minotari_mcp_wallet`, `minotari_mcp_node` | Model Context Protocol servers for AI agents. See [AI integration](#ai-integration-mcp-servers). |

The Aurora mobile wallets live in their own repositories:
[wallet-android](https://github.com/tari-project/wallet-android) and [wallet-ios](https://github.com/tari-project/wallet-ios).

## Installing from binaries

[Download binaries](https://tari.com/downloads/) from [tari.com](https://www.tari.com/). This is the easiest way to run
a Tari node, but you're essentially trusting the person that built and uploaded them that nothing untoward has happened.

Hashes of the binaries are published alongside the downloads. Get the hash of your download with:

(\*nix)

    shasum -a256 <PATH_TO_BINARY_INSTALL_FILE>

(Windows)

    certUtil -hashfile <PATH_TO_BINARY_INSTALL_FILE> SHA256

If the result doesn't match the published hash, don't run the binary. Note that this only checks that your binary was
downloaded correctly; it cannot detect if the binary was replaced by a bad actor. If you need to ensure that your binary
matches the source, see [Building from source](#building-from-source) below.

The installer lays down soft links/shortcuts for each application. If you want to mine, use:

- SHA-3 standalone mining — `start_minotari_miner`
- Merge mining with Monero — `start_minotari_merge_mining_proxy`, then `start_xmrig`

On Windows the base node needs a Tor daemon; start it with `start_tor` before starting the node. The Tor console prints
`[notice] Bootstrapped 100% (done): Done` once it is ready. On Linux and macOS Tor is embedded in the binaries via the
`libtor` feature, so no separate Tor process is required.

## Building from source

### Requirements

- Rust `1.93.0` or newer (the workspace uses edition 2024). `rust-toolchain.toml` pins the `stable` channel, so
  `rustup` will pick the right toolchain automatically.
- A recent `protoc` (protobuf compiler). Distro packages are often too old to compile the `optional` proto3 fields used
  here — see the note in `scripts/install_ubuntu_dependencies.sh`.

#### Linux (Ubuntu/Debian)

The dependency list CI uses is kept in a script, so it never drifts from what actually builds:

```bash
sudo apt-get update
sudo bash scripts/install_ubuntu_dependencies.sh
```

#### macOS

```bash
brew update
brew install cmake coreutils automake autoconf libtool protobuf zip
```

#### Windows

Follow the guide in [buildtools/windows-dev-environment-notes.md](buildtools/windows-dev-environment-notes.md) to set
up your build environment.

### Choosing a network

When compiling from source you select the target network at compile time with the `TARI_TARGET_NETWORK` environment
variable. This decides which consensus rules and feature flags are compiled in:

| `TARI_TARGET_NETWORK` | Default network | Networks you can run |
| --- | --- | --- |
| unset | esmeralda | esmeralda, igor, localnet |
| `testnet` | esmeralda | esmeralda, igor, localnet |
| `nextnet` | nextnet | nextnet |
| `mainnet` | mainnet | mainnet, stagenet |

For example, to build for the Esmeralda testnet:

```bash
TARI_TARGET_NETWORK=testnet cargo build --release
```
If you do not specify a target network, it will choose testnet. 

At runtime you pick the specific network with `--network <name>` or the `TARI_NETWORK` environment variable, within the
set the binary was compiled for.

### Build

(\*nix)

    cd tari
    cargo build --release

(Windows)

This is similar to building on Linux, except the Microsoft Visual Studio environment must be sourced. Open the
appropriate _x64\x86 Native Tools Command Prompt for VS_, and in your main Tari directory run the build:

    cd %USERPROFILE%\Code\tari
    cargo build --release

Compiled executables can be found at these paths:

    ./target/release/minotari_node
    ./target/release/minotari_console_wallet
    ./target/release/minotari_merge_mining_proxy
    ./target/release/minotari_miner
    ./target/release/minotari_offline_signer
    ./target/release/minotari_mcp_wallet
    ./target/release/minotari_mcp_node

Alternatively, `cargo` can build and install an executable into `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on Windows),
so it is runnable from anywhere on your system:

    cargo install --path=applications/minotari_node --force
    cargo install --path=applications/minotari_console_wallet --force
    cargo install --path=applications/minotari_merge_mining_proxy --force
    cargo install --path=applications/minotari_miner --force
    cargo install --path=applications/minotari_offline_signer --force
    cargo install --path=applications/minotari_mcp_wallet --force
    cargo install --path=applications/minotari_mcp_node --force

## Running

Run the executables from `./target/release` (or from `~/.cargo/bin` if you used `cargo install`):

    minotari_node

    minotari_console_wallet

    minotari_merge_mining_proxy

    minotari_miner

    minotari_offline_signer

Or run them from the source directory with `cargo`. Omit `--release` to run a debug build:

    cargo run --bin minotari_node --release

    cargo run --bin minotari_console_wallet --release

    cargo run --bin minotari_merge_mining_proxy --release

    cargo run --bin minotari_miner --release

With the default options, the blockchain database, wallet databases, log files and all configuration files are created
under `~/.tari/<network>` (Linux/macOS) or `%USERPROFILE%\.tari\<network>` (Windows). Override the root with
`--base-path <base-path>`.

The base node's gRPC server listens on `127.0.0.1:18142` and is enabled by default. The console wallet's gRPC server
listens on `127.0.0.1:18143` and is off by default — enable it with `--grpc-enabled`.

### Base node console

The base node runs an interactive console. Type `help` to list the available commands, or press Tab for
auto-completion:

```
>> get-chain-metadata
Best block height: 5228
Total accumulated difficulty: 5892870
Best block hash: 2c4f92854b2160324b8afebaa476b39be4004d2a7a19c69dd2d4e4da257bfee2
Pruning horizon: 0
Pruned height: 0
>> get-state-info
Current state machine state:
Synchronizing blocks: Syncing from the following peers:
510c83279adc7cb7d7dda0aa07
Syncing 5229/5233
```

Pass `--non-interactive-mode` to run the node without the console.

## Using Docker

Images are published to `ghcr.io/tari-project` and `quay.io/tarilabs` for `minotari_node`,
`minotari_console_wallet`, `minotari_merge_mining_proxy`, `minotari_sha3_miner` and `minotari_offline_signer`.

Using `compose.yaml`:

```yaml
services:
  minotari_node:
    image: ghcr.io/tari-project/minotari_node:latest-nextnet
    restart: unless-stopped
    volumes:
      - ./data:/var/tari
    # Required for an interactive session
    stdin_open: true
    tty: true
    # The image defaults to --non-interactive-mode; clear it to get the console
    command: []
    ports:
      - "18189:18189"
```

The image runs with `/var/tari` as the Tari home directory. Mounting the host `./data` directory there persists the
node base path (`/var/tari/node`) and config path (`/var/tari/config/config.toml`).

Port `18189` is the P2P TCP listener. The gRPC port (`18142`) binds to `127.0.0.1` inside the container by default, so
publishing it also requires overriding `base_node.grpc_address` to listen on `0.0.0.0`.

Start the service with `docker compose up -d` and check it with `docker compose ps`. To reach the console, find the
container ID with `docker ps` and run `docker attach <container-id>`.

## AI integration (MCP servers)

Tari provides [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-03-26) servers that let AI
agents interact with Tari over stdio.

- **`minotari_mcp_wallet`** — wallet operations, read-only unless control operations are enabled
- **`minotari_mcp_node`** — blockchain and node queries
- **`minotari_mcp_common`** — shared infrastructure for building the MCP servers

Both servers talk to a console wallet or base node over local gRPC, and will auto-launch that process if it isn't
already running (`--auto-launch-wallet` / `--auto-launch-node`, on by default). To point them at an already-running
instance instead, start it with gRPC enabled:

```bash
minotari_console_wallet --grpc-enabled
minotari_mcp_wallet --wallet-grpc-address 127.0.0.1:18143

minotari_node   # gRPC is enabled by default
minotari_mcp_node --node-grpc-address 127.0.0.1:18142
```

Security-relevant options:

- Read-only by default; `--mcp-control-enabled` opts in to operations that can move funds or change node state
- `--require-confirmation` (wallet) prompts the user for every value transfer
- `--mcp-rate-limit` caps requests per minute per client
- `--mcp-audit-logging` / `--mcp-audit-log-path` record every operation
- The servers connect only to local gRPC endpoints

For complete documentation, see:

- [MCP Implementation Guide](docs/mcp/TARI_MCP_IMPLEMENTATION.md)
- [Wallet MCP Server](applications/minotari_mcp_wallet/README.md)
- [Node MCP Server](applications/minotari_mcp_node/README.md)
- [Common MCP Framework](applications/minotari_mcp_common/README.md)

## Development

Want to contribute? Start with the [Contributing Guide](Contributing.md) and the
[Reviewing Guide](docs/src/reviewing_guide.md).
Want to discuss new ideas? Head over to the [Tari Forum](https://community.tari.com/).
Want the technical details? Head over to the [Tari RFCs](https://rfc.tari.com/).

Security issues should be reported as described in [SECURITY.md](SECURITY.md).

### Unit tests

Unit tests run under [nextest](https://nexte.st/):

```bash
cargo install cargo-nextest
cargo ci-test
```

### Integration tests

The cucumber integration tests live in [`integration_tests`](integration_tests/README.md) and need the release binaries
to be built first:

```bash
cargo build --release
cargo test --release --test cucumber --all-features --package tari_integration_tests -- -t "@critical"
```

`cargo ci-cucumber` is a shorthand for the critical subset.

### Formatting and lints

Formatting uses nightly-only rustfmt options, so it must be run on a nightly toolchain. Clippy runs through
[cargo-lints](https://crates.io/crates/cargo-lints) using the rule set in `lints.toml`:

```bash
cargo install cargo-lints
cargo +nightly ci-fmt        # check formatting
cargo +nightly ci-fmt-fix    # apply formatting
cargo ci-clippy              # lints
cargo ci-check               # fast type check
```

All `ci-*` aliases are defined in [`.cargo/config.toml`](.cargo/config.toml).

## Project documentation

- [RFC documents](https://rfc.tari.com) — protocol specifications, maintained in the
  [rfcs repository](https://github.com/tari-project/rfcs)
- [`docs/`](docs/README.md) — developer docs as an mdbook; run `mdbook serve` in `docs/` and open
  [localhost:3000](http://localhost:3000)
- [Applications overview](docs/guide/applications_overview.md) — every application, its CLI options and config overrides
- [gRPC overview](docs/guide/grpc_overview.md) and [FFI overview](docs/guide/ffi_overview.md)

### Source code documentation

Run

    cargo doc

to generate the documentation. The generated HTML sits in `target/doc/`. To open a specific package's documentation
directly in your browser, run:

    cargo doc -p <package> --open
