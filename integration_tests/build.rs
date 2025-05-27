// Copyright 2022. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::env;

fn main() {
    // link FFI lib
    #[cfg(target_os = "macos")]
    let out_dirs = env::var("DYLD_FALLBACK_LIBRARY_PATH").unwrap();
    #[cfg(windows)]
    let out_dirs = env::var("PATH").unwrap();
    #[cfg(target_os = "linux")]
    let out_dirs = env::var("LD_LIBRARY_PATH").unwrap();

    let out_dir = out_dirs.split(':').next().unwrap_or(".");
    println!("cargo::rustc-link-search=native={}", out_dir);
    println!("cargo::rustc-link-lib=dylib=minotari_wallet_ffi");
}
