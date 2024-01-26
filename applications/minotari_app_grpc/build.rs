// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().build_client(true).build_server(true).compile(
        &[
            "proto/base_node.proto",
            "proto/wallet.proto",
            "proto/validator_node.proto",
        ],
        &["proto"],
    )?;

    Ok(())
}
