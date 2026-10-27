// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_features::resolver::build_features;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_features();
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        // Required so that older `protoc` versions (e.g. the one shipped for riscv64 linux) accept the
        // proto3 `optional` keyword instead of failing the build.
        .protoc_arg("--experimental_allow_proto3_optional")
        .skip_debug("tari.rpc.UnblindedOutput")
        .skip_debug("tari.rpc.ImportUtxosRequest")
        .skip_debug("tari.rpc.CreateBurnTransactionRequest")
        .skip_debug("tari.rpc.RegisterValidatorNodeRequest")
        .skip_debug("tari.rpc.SubmitValidatorNodeExitRequest")
        .compile_protos(
            &["proto/base_node.proto", "proto/wallet.proto", "proto/p2pool.proto"],
            &["proto"],
        )?;

    Ok(())
}
