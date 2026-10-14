// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Placeholder workspace member that does nothing at runtime.
//!
//! It was originally meant to pull in every workspace dependency so that `cargo build --bin deps_only` could warm a
//! shared Docker layer cache. It has only ever depended on `log`, so building it compiles nothing else, and no
//! Dockerfile or CI job builds it. `buildtools/docker_rig/tarilabs.Dockerfile` copies this directory only so the
//! workspace manifest resolves inside the image.
//!
//! The one `log::info!` below never prints: no logger is initialised.

fn main() {
    log::info!("Hi Tari. This app does nothing except build all the dependencies");
}
