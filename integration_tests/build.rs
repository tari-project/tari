// Copyright 2022. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::{
    env,
    path::{Path, PathBuf},
};

/// Optional override for the directory that holds the `minotari_wallet_ffi` shared library.
const FFI_LIB_DIR_ENV: &str = "MINOTARI_WALLET_FFI_LIB_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={FFI_LIB_DIR_ENV}");

    // The cucumber tests load the wallet FFI as a shared library, exactly as a mobile client would, so it keeps its own
    // copy of every global (logger, tokio runtime context, ...). Linking the crate statically would share those with
    // the test harness and abort the FFI scenarios.
    //
    // Cargo only places a cdylib at a predictable path when it is built as a root unit, i.e.
    // `cargo build [--release] -p minotari_wallet_ffi`, which copies it into the profile's artifact directory
    // (`target/<profile>/`). Since Cargo 1.100 the per-crate outputs of a dependency live in hashed directories
    // (`target/<profile>/build/<crate>/<hash>/out/`) that a build script cannot predict, so we no longer look there.
    let lib_dirs = match env::var_os(FFI_LIB_DIR_ENV) {
        Some(dir) => vec![PathBuf::from(dir)],
        None => {
            let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
            let profile_dir = profile_dir(&out_dir);
            // `target/<profile>/deps` is where Cargo < 1.100 left dependency cdylibs.
            vec![profile_dir.to_path_buf(), profile_dir.join("deps")]
        },
    };

    let lib_file = ffi_lib_file_name();
    for dir in &lib_dirs {
        println!("cargo:rerun-if-changed={}", dir.join(&lib_file).display());
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    if !lib_dirs.iter().any(|dir| dir.join(&lib_file).exists()) {
        println!(
            "cargo:warning={lib_file} not found in {}; build it first with `cargo build [--release] --locked -p \
             minotari_wallet_ffi` (same profile as the tests) or set {FFI_LIB_DIR_ENV}",
            lib_dirs
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    println!("cargo:rustc-link-lib=minotari_wallet_ffi");
}

/// `OUT_DIR` is `<target>/<profile>/build/<crate>-<hash>/out` (Cargo < 1.100) or
/// `<target>/<profile>/build/<crate>/<hash>/out` (Cargo >= 1.100); the profile directory is the parent of `build`.
fn profile_dir(out_dir: &Path) -> &Path {
    out_dir
        .ancestors()
        .find(|p| p.file_name().is_some_and(|name| name == "build"))
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("unexpected OUT_DIR layout: {}", out_dir.display()))
}

fn ffi_lib_file_name() -> String {
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => "minotari_wallet_ffi.dll.lib".to_string(),
        Ok("macos") | Ok("ios") => "libminotari_wallet_ffi.dylib".to_string(),
        _ => "libminotari_wallet_ffi.so".to_string(),
    }
}
