// Copyright 2022. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // OUT_DIR =
    // target/release/build/<integration-test-crate>/out
    //
    // ancestors:
    // 0 = out
    // 1 = <crate>
    // 2 = build
    // 3 = release
    // 4 = target
    let deps_dir = out_dir
        .ancestors()
        .nth(3)
        .unwrap()
        .join("deps");

    println!("cargo:rustc-link-search=native={}", deps_dir.display());
    println!("cargo:rustc-link-lib=minotari_wallet_ffi");
}
