[![Waffle.io - Columns and their card count](https://badge.waffle.io/tari-project/tari.svg?columns=Inbox,Backlog,In%20Progress,Review,Done)](https://waffle.io/tari-project/tari)

# The Tari protocol

## Documentation

* [RFC documents](https://rfc.tari.com) are hosted on Github Pages. The source markdown is in the `RFC` directory.
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

See [RFC-0110/CodeStructure](./RFC/src/RFC-0010_CodeStructure.md) for details on the code structure and layout.

## Conversation channels

[<img src="https://ionicons.com/ionicons/svg/md-paper-plane.svg" width="32">](https://t.me/tarilab) Non-technical discussions and gentle sparring.

[<img src="https://ionicons.com/ionicons/svg/logo-reddit.svg" width="32">](https://reddit.com/r/tari/) Forum-style Q&A
and other Tari-related discussions.

[<img src="https://ionicons.com/ionicons/svg/logo-twitter.svg" width="32">](https://twitter.com/tari) Follow @tari to be
the first to know about important updates and announcements about the project.

Most of the technical conversation about Tari development happens on [#FreeNode IRC](https://freenode.net/) in the #tari-dev room.
