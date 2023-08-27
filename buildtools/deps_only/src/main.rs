// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

//! This is a dummy application that does nothing useful. BUT building it will download and compile all the crates if
//! you run `cargo build --bin deps_only` from the project root. This is very handy for creating a common image in
//! docker that provides a common shared set of compiled libraries, which can be cached and used to build a variety
//! of other app-specific images

fn main() {
    log::info!("Hi Taiji. This app does nothing except build all the dependencies");
}
