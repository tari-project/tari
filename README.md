# The Tari protocol

## Documentation

* [RFC documents](https://tari-project.github.io/tari/) are hosted on Github Pages. The source markdown is in the `RFC`
  directory.
* Source code documentation is hosted on [docs.rs](https://docs.rs)

### Serving the documentation locally

#### RFC documents

Firstly, install `mdbook`. Assuming you have Rust and cargo installed, run

    cargo install mdbook

Then, from the `RFC` directory, run

    mdbook serve

and the RFC documentation will be available at http://localhost:3000.

#### Source code documentation

Run

    cargo doc

to generate the documentation. The generated html sits in `target/doc/`. Alternatively, to open a specific package's documentation directly in your browser, run

    cargo doc -p <package> --open

## Code organisation

The code follows a domain-driven design layout, with top-level folders falling into infrastructure, domain, or
application layers.

The `infrastructure` folder contains code that is not Tari-specific. It holds the following crates:
* `comms`: The networking and messaging subsystem
* `crypto`: All cryptographic services, including a Curve25519 implementation
* `storage`: Data persistence services, including LMDB

The `base_layer` is a domain-level folder and contains:
* `core`: common classes and traits, such as `Transaction`s and `Block`s
* `blockchain`: The Tari consensus code
* `mempool`: The unconfirmed transaction pool implementation
* `mining`: The merge-mining modules
* `p2p`: The block and transaction propagation module
* `api`: interfaces for clients and wallets to interact with the base layer components

The `digital_assets_layer` is a domain-level folder contains code related to the management of native Tari digital
assets. Substructure TBD.

## Conversation channels

[<img src="https://ionicons.com/ionicons/svg/logo-github.svg" width="32">](https://github.com/tari-project/tari) You are
here.

[<img src="https://ionicons.com/ionicons/svg/md-paper-plane.svg" width="32">](https://t.me/tarilab) Non-technical discussions and gentle sparring.

[<img src="https://ionicons.com/ionicons/svg/logo-reddit.svg" width="32">](https://reddit.com/r/tari/) Forum-style Q&A
and other Tari-related discussions.

[<img src="https://ionicons.com/ionicons/svg/logo-twitter.svg" width="32">](https://twitter.com/tari) Follow @tari to be
the first to know about important updates and announcements about the project.

Most of the technical conversation about Tari development happens on [#FreeNode IRC](https://freenode.net/) in the #tari-dev room.
