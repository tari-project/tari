// Copyright 2022. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::env;

fn main() {
    // link FFI lib
    // the FFI library is built by the `minotari_wallet_ffi` crate
    // and is expected to be in the folder: `target/<profile>/deps/`
    // where `<profile>` is the current build profile (e.g., debug, release)
    // We need to tell the linker to look in that folder for the library.
    // All OS's have different ENV variables for this, so we handle them separately.
    // the target/<profile>/deps/ is the first one, so we take that one out
    #[cfg(target_os = "macos")]
    let out_dirs = env::var("DYLD_FALLBACK_LIBRARY_PATH").unwrap();
    #[cfg(windows)]
    let out_dirs = env::var("PATH").unwrap();
    #[cfg(target_os = "linux")]
    let out_dirs = env::var("LD_LIBRARY_PATH").unwrap();

    #[cfg(windows)]
    let out_dir = out_dirs.split(';').next().unwrap_or(".");
    #[cfg(not(windows))]
    let out_dir = out_dirs.split(':').next().unwrap_or(".");
    println!("cargo::rustc-link-search=native={}", out_dir);
    println!("cargo::rustc-link-lib=dylib=minotari_wallet_ffi");
}
