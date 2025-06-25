// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_features::resolver::build_features;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_features();
    // // Tell Cargo to rerun the build script if any .proto files change
    // println!("cargo:rerun-if-changed=proto/");

    // // Or be more specific for each proto file
    // println!("cargo:rerun-if-changed=proto/base_node.proto");
    // println!("cargo:rerun-if-changed=proto/wallet.proto");
    // println!("cargo:rerun-if-changed=proto/validator_node.proto");
    // println!("cargo:rerun-if-changed=proto/p2pool.proto");

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(
            &[
                "proto/base_node.proto",
                "proto/wallet.proto",
                "proto/validator_node.proto",
                "proto/p2pool.proto",
            ],
            &["proto"],
        )?;

    Ok(())
}
