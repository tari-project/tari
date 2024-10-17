//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::path::{Path, PathBuf};

use pb_rs::{types::FileDescriptor, ConfigBuilder};

const PROTOS: &[&str] = &["messages.proto"];

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir).join("proto");

    let in_dir = PathBuf::from(::std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("proto");
    // Re-run this build.rs if the protos dir changes (i.e. a new file is added)
    println!("cargo:rerun-if-changed={}", in_dir.to_str().unwrap());
    for proto in PROTOS {
        println!("cargo:rerun-if-changed={}", in_dir.join(proto).to_str().unwrap());
    }

    // Delete all old generated files before re-generating new ones
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir).unwrap();
    }
    std::fs::DirBuilder::new().create(&out_dir).unwrap();
    let protos = PROTOS.iter().map(|p| in_dir.join(p)).collect::<Vec<_>>();
    let config_builder = ConfigBuilder::new(&protos, None, Some(&out_dir), &[in_dir])
        .unwrap()
        .dont_use_cow(true);
    FileDescriptor::run(&config_builder.build()).unwrap()
}
