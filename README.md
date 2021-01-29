[![Build](https://circleci.com/gh/tari-project/tari.svg?style=svg)](https://circleci.com/gh/tari-project/tari)

# The Tari protocol

## Installing the base node software

### Using binaries

[Download binaries from tari.com](https://tari.com/downloads). This is the easiest way to run a Tari node, but you're
essentially trusting the person that built and uploaded them that nothing untoward has happened.

We've tried to limit the risks by publishing [hashes of the binaries](https://tari.com/downloads) on our website.

You can check that the binaries match the hash by running

    sha256sum path/to/tari_base_node

## Building from source

To build the Tari codebase from source, there are a few dependencies you need to have installed.

### Install development packages 

First you'll need to make sure you have a full development environment set up:

#### (macOS)

```
brew update
brew install cmake openssl tor ncurses coreutils
brew install --cask powershell
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

First you'll need to make sure you have a full development environment set up:

- git
  
- https://git-scm.com/downloads
  
- LLVM
  - https://releases.llvm.org/
  - Create a `LIBCLANG_PATH` environment variable pointing to the LLVM lib path, e.g. 
    ```
    setx LIBCLANG_PATH "C:\Program Files\LLVM\lib"
    ```

- Build Tools
  - Microsoft Visual Studio Version 2019 or later 
    - C++ CMake tools for Windows
    - MSVC build tools (latest version for your platform ARM, ARM64 or x64.x86)
    - Spectre-mitigated libs (latest version for your platform ARM, ARM64 or x64.x86)

   or

  - [CMake](https://cmake.org/download/)
  - [Build Tools for Visual Studio 2019](
https://visualstudio.microsoft.com/thank-you-downloading-visual-studio/?sku=BuildTools&rel=16)

- SQLite:
  - Download 32bit/64bit Precompiled Binaries for Windows for [SQL Lite](https://www.sqlite.org/index.html) and unzip 
    to local path, e.g. `%USERPROFILE%\.sqlite`
  - Open the appropriate x64\x86 `Native Tools Command Prompt for VS 2019` in `%USERPROFILE%\.sqlite`
    - Run either of these, depending on your environment (32bit/64bit):
      ```
      lib /DEF:sqlite3.def /OUT:sqlite3.lib /MACHINE:x64
      ```
      ```
      lib /DEF:sqlite3.def /OUT:sqlite3.lib /MACHINE:x86
      ```
  - Ensure the directory containing `sqlite3.dll`, e.g. `%USERPROFILE%\.sqlite`, is in the path
  - Create a `SQLITE3_LIB_DIR` environment variable pointing to the SQLite lib path, e.g. 
    ```
    setx SQLITE3_LIB_DIR "%USERPROFILE%\.sqlite"
    ```
- OpenSSL:
  - Download full version of the 64bit Precompiled Binaries for Windows for
    [OpenSSL](https://slproweb.com/products/Win32OpenSSL.html)
  - Install using all the default prompts
  
    **Note**: It is important that the dlls are available in the path. To test:
    ```
    where libcrypto-1_1-x64.dll
    where libssl-1_1-x64.dll
    ``` 

- Tor
  - Donwload [Tor Windows Expert Bundle](https://www.torproject.org/download/tor/)
  - Extract to local path, e.g. `C:\Program Files (x86)\Tor Services`
  - Ensure the directory containing the Tor executable, e.g. `C:\Program Files (x86)\Tor Services\Tor`, is in the path


#### Install Rust (*nix)

You can follow along at [The Rust Website](https://www.rust-lang.org/tools/install) or just follow these steps to get
Rust installed on your machine.

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

Then make sure that `cargo` has been added to your path.

    export PATH="$HOME/.cargo/bin:$PATH"

#### Install Rust (Windows 10)

Follow the installation process for Windows at [The Rust Website](https://www.rust-lang.org/tools/install). Then make 
sure that `cargo` and `rustc` has been added to your path:

    cargo --version
    rustc --version

#### Checkout the source code

In your directory of choice (e.g. `%USERPROFILE%\Code` on Windows), clone the Tari repo

    git clone https://github.com/tari-project/tari.git


#### Build

Grab a cup of coffee and begin the Tari build

 (*nix)

    cd tari
    cargo build --release

(Windows)

This is similar to building in Ubuntu, except the Microsoft Visual Studio environment must be sourced. Open the 
appropriate _x64\x86 Native Tools Command Prompt for VS 2019_, and in your main Tari directory perform the 
build, which will create the executable inside your `%USERPROFILE%\Code\tari\target\release` directory:

    cd %USERPROFILE%\Code\tari
    cargo build --release

A successful build should output something as follows
```
   Compiling tari_wallet v0.0.9 (.../tari/base_layer/wallet)
   Compiling test_faucet v0.0.1 (.../tari/applications/test_faucet)
   Compiling tari_wallet_ffi v0.0.9 (.../tari/base_layer/wallet_ffi)
   Compiling tari_base_node v0.0.9 (.../tari/applications/tari_base_node)
    Finished release [optimized] target(s) in 12m 24s
```

Compiled executable can be found by following path:

    ./target/release/tari_base_node
    ./target/release/tari_console_wallet
    ./target/release/tari_merge_mining_proxy

Alternatively, cargo can build and install the executable into `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on Windows), so it will be executable from anywhere 
on your system.

    cargo install --path=applications/tari_base_node --force
    cargo install --path=applications/tari_console_wallet --force
    cargo install --path=applications/tari_merge_mining_proxy --force

---


Alternatively, cargo can build and install the executable into `%USERPROFILE%\.cargo\bin`, so it will be executable from
anywhere on your system.

    cargo install --path=applications/tari_base_node --force
    cargo install --path=applications/tari_console_wallet --force
    cargo install --path=applications/tari_merge_mining_proxy --force

### Running the Tari components

#### Base node

The executables will either be inside your `~/tari/target/release` (on Linux) or `%USERPROFILE%\Code\tari\target\release` 
(on Windows) directory, or alternatively, inside your `~/.cargo/bin` (on Linux) `%USERPROFILE%\.cargo\bin` (on Windows)
directory, depending on the build choice above, and must be run from the command line. If the former build method was 
used, you can run it from that directory, or you more likely want to copy it somewhere more convenient.

To run from any directory of your choice, where the executable is visible in the path (first time use):

    tari_base_node --init --create-id
    tari_base_node
    
    tari_console_wallet --init --create-id
    tari_console_wallet

Consecutive runs:

    tari_base_node
    
    tari_console_wallet
    
    tari_merge_mining_proxy

Alternatively, you can run the Tari components from your source directory using `cargo` (first time use):

    cargo run --bin tari_base_node --release --  --init --create-id
    cargo run --bin tari_base_node --release
    
    cargo run --bin tari_console_wallet --release --  --init --create-id
    cargo run --bin tari_console_wallet --release

Consecutive runs:

    cargo run --bin tari_base_node --release
    
    cargo run --bin tari_console_wallet --release
    
    cargo run --bin tari_merge_mining_proxy --release

Using all the default options, the blockchain database, wallet database, console wallet database, log files and all 
configuration files will be created in the `~/.tari` (on Linux) or `%USERPROFILE%\.tari` (on Windows) directory. 
Alternatively, by specifying `--base-path <base-path>` on the command line as well, all of this will be created in that 
directory.  

---

### Running the base node with a docker image

Docker images can be found at https://quay.io/repository/tarilabs/tari_base_node

Using ```docker-compose.yaml```
```
version: "3"

services:
  tari_base_node:
    image: quay.io/tarilabs/tari_base_node:v0.5.4
    restart: unless-stopped
    volumes:
      - ./data:/root/.tari
# These 2 params are required for an interactive docker-compose session
    stdin_open: true
    tty: true
    expose:
      - 18142
    ports:
      - "18142:18142"
```
Then run ```docker-compose up -d``` to start your docker service.  

Check the running state with ```docker-compose ps```
```
        Name           Command    State            Ports
------------------------------------------------------------------
tbn_tari_base_node_1   start.sh   Up      0.0.0.0:18142->18142/tcp
```
To connect to the console, use ```docker ps``` to get the container ID which to attach to the tari_base_node in docker
```
CONTAINER ID        IMAGE                                    COMMAND             CREATED             STATUS              PORTS                      NAMES
73427509a4bb        quay.io/tarilabs/tari_base_node:v0.5.4   "start.sh"          45 minutes ago      Up 26 minutes       0.0.0.0:18142->18142/tcp   tbn_tari_base_node_1
```
With the container ID ```73427509a4bb```, connect to the tari_base_node console as follows ```docker attach 73427509a4bb```
```
>> help
Available commands are:
help, version, get-balance, list-utxos, list-transactions, list-completed-transactions, cancel-transaction, send-tari, get-chain-metadata, list-peers, reset-offline-peers, ban-peer, unban-peer, list-connections, list-headers, check-db, calc-timing, discover-peer, get-block, search-utxo, search-kernel, search-stxo, get-mempool-stats, get-mempool-state, whoami, toggle-mining, get-mining-state, make-it-rain, coin-split, get-state-info, quit, exit
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
### Building a docker image

If you don't want to use the docker images provided by the community, you can roll your own!

First, clone the Tari repo
```bash
git clone git@github.com:tari-project/tari.git
```

Then build the image using the dockerfile in `buildtools`. The base node docker file build the application and then
places the binary inside a small container, keeping the executable binary to a minimum.

    docker build -t tari_base_node:latest -f ./buildtools/base_node.Dockerfile .

Test your image

    docker run --rm -ti tari_base_node tari_base_node --help

Run the base node

    docker run -ti -v /path/to/config/dir:/root/.tari tari_base_node

Default docker builds for base x86-64 CPU. Better performing builds can be created by passing build options

    docker build -t tari_base_node:performance --build-arg TBN_ARCH=skylake --build-arg TBN_FEATURES=avx2 -f ./buildtools/base_node.Dockerfile .

---

### Advanced build configurations

* [Building with Vagrant](https://github.com/tari-project/tari/issues/1407)

## Tari merge mining

In order to perform merge mining with Tari, the following components are needed:
- A Tari Base Node [_to supply blockchain metadata information_];
- A Tari Console Wallet [_to collect the Tari block rewards (coinbase transactions)_];
- The Tari Merge Mining Proxy [_to enable communication between all components_];
- XMRig [_to perform the mining_];
- Monero wallet (specifically a stagenet wallet address during testnet; the one provided can be used, or a custom 
  one can be set up) [_to collect Monero block rewards (coinbase transactions)_].

The Tari Merge Mining Proxy will be the communication gateway between all these components and will coordinate all 
activities. It will also submit finalized Tari and Monero blocks to the respective networks when RandomX is solved at 
the respective difficulties.

### Runtime prerequisites

The Tari Base Node, Tari Console Wallet and Tari Merge Mining Proxy can all run in the same directory, whereas XMRig will run in its own directory. By performing the default installation as described in [Installing the base node software](#installing-the-base-node-software), all these components will be available.

XMRig can also be build from sources. If that is your preference, follow these instructions: <https://xmrig.com/docs/miner/>.

### Configuration prerequisites

#### Tari components

The configuration prerequisites are the same for all three Tari components. After performing a 
[default installation](#installing-the-base-node-software), locate the main configuration file (`config.toml`), which 
will be created in the `~/.tari` (on Linux) or `%USERPROFILE%\.tari` (on Windows) directory. If the Windows installer 
was run, the main configuration file will be located in the installation directory as `config\config.toml`.

With the main configuration file, in addition to the settings already present, the following must also be enabled if 
they are not enabled already:

- For the Tari Base Node and the Tari Console Wallet, under section **`base_node.stibbons`**
  ```
  [base_node.stibbons]
  transport = "tor"
  allow_test_addresses = false
  grpc_enabled = true
  grpc_base_node_address = "127.0.0.1:18142"
  grpc_console_wallet_address = "127.0.0.1:18143"
  ```
- For the Tari Merge Mining Proxy, under section **`merge_mining_proxy.stibbons`**
  ```
  [merge_mining_proxy.stibbons]
  monerod_url = "http://18.133.55.120:38081"
  proxy_host_address = "127.0.0.1:7878"
  monerod_use_auth = false
  monerod_username = ""
  monerod_password = ""
  ```

None of the IP address + port combinations listed above should be in use otherwise. 

The `monerod_url` has to be set to a valid address (`host:port`) for `monerod` that is running Monero stagenet, which can 
be a [public node hosted by XMR.to](https://community.xmr.to/nodes.html), or to a local instance. To test if the address 
is working properly, try to paste `host:port/get_height` in an internet browser, example:

```
http://18.133.55.120:38081/get_height
```
A typical response would be:
```
{
  "hash": "faa4385c93c2d1c5c0af35140d25fcc37c2c2e13f50c7c415c78952e67ab15e7",
  "height": 701536,
  "status": "OK",
  "untrusted": false
}
```

_**Note:** A guide to setting up a local Monero stagenet on Linux can be found 
[here](https://github.com/tari-project/tari/blob/development/applications/tari_merge_mining_proxy/monero_stagenet_setup.md)._

#### Monero components

The XMRig configuration wizard at https://xmrig.com/wizard can be used to create the configuration file in JSON format:

- Start -> `+ New configuration`

- Pools -> `+ Add daemon`

  With `Add new daemon for Solo mining`, complete the required information, then `+ Add daemon`:

    - `Host, Port`: This must correspond to the `proxy_host_address` in the Tari configuration file.
​    - `Secure connection (TLS)`: `Uncheck`, Coin: `Monero`
​    - `Wallet address`: This can be any publicly available stagenet wallet address 
​      [as shown here](https://coin.fyi/news/monero/stagenet-wallet-8jyt89#!), or you can use your own.

- Backends -> Select `CPU` (`OpenCL` or `CUDA` also possible depending on your computer hardware)

- Misc -> With `Donate`, type in your preference

- Result -> With `Config file`, copy or download, than save as `config.json`.

Using the public stagenet wallet address, the resulting configuration will look like this:

```
{
    "autosave": true,
    "cpu": true,
    "opencl": false,
    "cuda": false,
    "pools": [
        {
            "coin": "monero",
            "url": "127.0.0.1:7878",
            "user": "55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt",
            "tls": false,
            "daemon": true
        }
    ]
}
```

Alternatively, these parameters can be passed in via the command line:

```
-o 127.0.0.1:7878 -u 55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt --coin monero --daemon
```

### Perform merge mining

The following components needed for merge mining must be started and preferably in this order:

- Tor:
  - Linux/OSX: Execute `start_tor.sh`.
  - Windows: `Start Tor Serviecs` menu item or `start_tor` shortcut in the Tari installation folder.

- Tari Base Node:
  - Linux/OSX: As per [Running the Tari components](#running-the-tari-components).
  - Windows: As per [Running the Tari components](#running-the-tari-components) or `Start Base Node` menu item or
    `start_tari_base_node` shortcut in the Tari installation folder.

- Tari Console Wallet:
  - Linux/OSX: As per [Running the Tari components](#running-the-tari-components).
  - Windows: As per [Running the Tari components](#running-the-tari-components) or `Start Console Wallet` menu item or
    `start_tari_console_wallet` shortcut in the Tari installation folder.

- Tari Merge Mining Proxy:
  - Linux/OSX: As per [Running the Tari components](#running-the-tari-components).
  - Windows: As per [Running the Tari components](#running-the-tari-components) or `Start Merge Mining Proxy` menu item 
    or `start_tari_merge_mining_proxy` shortcut in the Tari installation folder.

- XMRig
  - Configuration:
    - Ensure the `config.json` configuration file discussed in [Monero components](#monero-components) are copied to the 
      XMRig build or install folder, or, optionally pass in the command line parameters.
  - Runtime:
    - Linux/OSX: Execute `./xmrig` in the XMRig build or install folder.
    - Windows: Execute `xmrig` in the XMRig build or install folder, or `Start XMRig` menu item or `start_xmrig` 
      shortcut in the Tari installation folder.
      **Note**: On modern Windows versions, coin mining software is blocked by default, for example by Windows Defender. Ensure that these processes are allowed to run when challenged:
      - `PUA:Win32/CoinMiner`
      - `PUA:Win64/CoinMiner`
      - `App:XMRigMiner`

# Project documentation

* [RFC documents](https://rfc.tari.com) are hosted on Github Pages. The source markdown is in the `RFC` directory.
* Source code documentation is hosted on [docs.rs](https://docs.rs)

## RFC documents

The RFCs are long-form technical documents proposing changes and features to the Tari network and ecosystem. They are hosted at https://rfc.tari.com, but you can easily build and serve alocal version yourself.

Firstly, install `mdbook`. Assuming you have Rust and cargo installed, run

    cargo install mdbook

Then, from the `RFC` directory, run

    mdbook serve

and the RFC documentation will be available at http://localhost:3000.

### Source code documentation

Run

    cargo doc

to generate the documentation. The generated html sits in `target/doc/`. Alternatively, to open a specific package's documentation directly in your browser, run

    cargo doc -p <package> --open

## Code organisation

See [RFC-0110/CodeStructure](./RFC/src/RFC-0010_CodeStructure.md) for details on the code structure and layout.

## Conversation channels

[<img src="https://ionicons.com/ionicons/svg/md-paper-plane.svg" width="32">](https://t.me/tarilab) Non-technical discussions and gentle sparring.

[<img src="https://ionicons.com/ionicons/svg/logo-reddit.svg" width="32">](https://reddit.com/r/tari/) Forum-style Q&A
and other Tari-related discussions.

[<img src="https://ionicons.com/ionicons/svg/logo-twitter.svg" width="32">](https://twitter.com/tari) Follow @tari to be
the first to know about important updates and announcements about the project.

Most of the technical conversation about Tari development happens on [#FreeNode IRC](https://freenode.net/) in the #tari-dev room.
