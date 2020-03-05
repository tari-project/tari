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

### Building from Source (Ubuntu 18.04)

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

#### Run

The executable is currently inside your `target/release` folder. You can run it from that folder if you like, but you'll
more likely want to copy it somewhere more convenient. You can simply run

    cargo install -p tari_base_node [![Build](https://circleci.com/gh/tari-project/tari.svg?style=svg)](https://circleci.com/gh/tari-project/tari)

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

### Building from Source (Ubuntu 18.04)

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

#### Run

The executable is currently inside your `target/release` folder. You can run it from that folder if you like, but you'll
more likely want to copy it somewhere more convenient. You can simply run

    cargo install -p tari_base_node

and cargo will copy the executable into `~/.cargo/bin`. This folder was added to your path in a previous step, so it
will be executable from anywhere on your system.

Alternatively, you can run the node from your source folder with the command

    cargo run -p tari_base_node

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


and cargo will copy the executable into `~/.cargo/bin`. This folder was added to your path in a previous step, so it
will be executable from anywhere on your system.

Alternatively, you can run the node from your source folder with the command

    cargo run -p tari_base_node

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

Our community has contr

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
