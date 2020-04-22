[![Build](https://circleci.com/gh/tari-project/tari.svg?style=svg)](https://circleci.com/gh/tari-project/tari)

# The Tari protocol

## Installing the base node software

### Using binaries

[Download binaries from tari.com](https://tari.com/downloads). This is the easiest way to run a Tari node, but you're
essentially trusting the person that built and uploaded them that nothing untoward has happened.

We've tried to limit the risks by publishing [hashes of the binaries](https://tari.com/downloads) on our website.

You can check that the binaries match the hash by running

    sha256sum path/to/tari_base_node

### Running a node in Docker

If you have docker on your machine, you can run a prebuilt node using one of the docker images on
[quay.io](https://quay.io/user/tarilabs).

### Building from source (Ubuntu 18.04)

To build the Tari codebase from source, there are a few dependencies you need to have installed.


#### Install development packages

First you'll need to make sure you have a full development environment set up:

```
sudo apt-get -y install openssl libssl-dev pkg-config libsqlite3-dev clang git cmake libc++-dev libc++abi-dev
```

#### Install Rust

You can follow along at [The Rust Website](https://www.rust-lang.org/tools/install) or just follow these steps to get
Rust installed on your machine.

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

Then make sure that `cargo` has been added to your path.

    export PATH="$HOME/.cargo/bin:$PATH"

#### Checkout the source code

In your folder of choice, clone the Tari repo

    git clone https://github.com/tari-project/tari.git


#### Build

Grab a cup of coffee and begin the Tari build

    cd tari
    cargo build --release

A successful build should output something as follows
```
   Compiling tari_wallet v0.0.9 (.../tari/base_layer/wallet)
   Compiling test_faucet v0.0.1 (.../tari/applications/test_faucet)
   Compiling tari_wallet_ffi v0.0.9 (.../tari/base_layer/wallet_ffi)
   Compiling tari_base_node v0.0.9 (.../tari/applications/tari_base_node)
    Finished release [optimized] target(s) in 12m 24s
```

Alternatively, cargo can build and install the executable into `~/.cargo/bin`, so it will be executable from anywhere 
on your system.

    cargo install tari_base_node

### Building from source (Windows 10)

To build the Tari codebase from source on Windows 10, there are a few dependencies you need to have installed.

_**Note:** The Tari codebase does not work in Windows Subsystem for Linux version 1 (WSL 1), as the low-level calls 
used by LMBD breaks it. Compatibility with WSL-2 must still be tested in future when it is released in a stable 
Windows build._

#### Install dependencies

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
  - Ensure folder containing `sqlite3.dll`, e.g. `%USERPROFILE%\.sqlite`, is in the path
  - Create a `SQLITE3_LIB_DIR` environment variable pointing to the SQLite lib path, e.g. 
    ```
    setx SQLITE3_LIB_DIR "%USERPROFILE%\.sqlite"
    ```

- Tor
  - Donwload [Tor Windows Expert Bundle](https://www.torproject.org/download/tor/)
  - Extract to local path, e.g. `C:\Program Files (x86)\Tor Services`
  - Ensure folder containing the Tor executable, e.g. `C:\Program Files (x86)\Tor Services\Tor`, is in the path

#### Install Rust

Follow the installation process for Windows at [The Rust Website](https://www.rust-lang.org/tools/install). Then make 
sure that `cargo` and `rustc` has been added to your path:

    cargo --version
    rustc --version

#### Checkout the source code

In your folder of choice, e.g. `%USERPROFILE%\Code`, clone the Tari repo

    git clone https://github.com/tari-project/tari.git


#### Build

This is similar to [building in Ubuntu](#building-from-source-ubuntu-1804), except the Microsoft Visual Studio 
environment must be sourced.

Open the appropriate _x64\x86 Native Tools Command Prompt for VS 2019_, and in your main Tari folder perform the 
build, which will create the executable inside your `%USERPROFILE%\Code\tari\target\release` folder:

    cd %USERPROFILE%\Code\tari
    cargo build --release

A successful build should output something as follows
```
   Compiling tari_wallet v0.0.9 (...\tari\base_layer\wallet)
   Compiling test_faucet v0.0.1 (...\tari\applications\test_faucet)
   Compiling tari_wallet_ffi v0.0.9 (...\tari\base_layer\wallet_ffi)
   Compiling tari_base_node v0.0.9 (...\tari\applications\tari_base_node)
    Finished release [optimized] target(s) in 12m 24s
```

Alternatively, cargo can build and install the executable into `%USERPROFILE%\.cargo\bin`, so it will be executable from 
anywhere on your system.

    cargo install tari_base_node

### Running the base node

The executable will either be inside your `~/tari/target/release` (on Linux) or `%USERPROFILE%\Code\tari\target\release` 
(on Windows) folder, or alternatively, inside your `~/.cargo/bin` (on Linux) `%USERPROFILE%\.cargo\bin` (on Windows)
folder, depending on the build choice above, and must be run from the command line. If the former build method was used, 
you can run it from that folder, or you more likely want to copy it somewhere more convenient.

To run from any folder of your choice, where the executable is visible in the path (first time use):

    tari_base_node --init --create-id
    tari_base_node

Consecutive runs:

    tari_base_node

Alternatively, you can run the node from your source folder using `cargo` (first time use):

    cargo run --bin tari_base_node --release --  --init --create-id
    cargo run --bin tari_base_node --release

Consecutive runs:

    cargo run --bin tari_base_node --release

Using all the default options, the blockchain database, wallet database, log files and all configuration files will be 
created in the `~/.tari` (on Linux) or `%USERPROFILE%\.tari` (on Windows) folder. Alternatively, by specifying 
`--base-path <base-path>` on the command line as well, all of this will be created in that folder.  

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

    docker run tari_base_node tari_base_node --help

### Advanced build configurations

* [Building with Vagrant](https://github.com/tari-project/tari/issues/1407)

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
