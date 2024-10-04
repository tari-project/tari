// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_features::resolver::build_features;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_features();
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
