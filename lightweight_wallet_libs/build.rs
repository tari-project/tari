// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::env;
use std::path::PathBuf;

fn main() {
    // Only build GRPC code if the feature is enabled
    if cfg!(feature = "grpc") {
        build_grpc();
    }
}

fn build_grpc() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    // Define the proto files to compile
    let proto_files = [
        "../applications/minotari_app_grpc/proto/types.proto",
        "../applications/minotari_app_grpc/proto/transaction.proto", 
        "../applications/minotari_app_grpc/proto/block.proto",
        "../applications/minotari_app_grpc/proto/network.proto",
        "../applications/minotari_app_grpc/proto/sidechain_types.proto",
        "../applications/minotari_app_grpc/proto/base_node.proto",
    ];

    // Configure tonic build
    tonic_build::configure()
        .build_server(false) // We only need the client
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("tari_descriptor.bin"))
        .compile(&proto_files, &["../applications/minotari_app_grpc/proto"])
        .unwrap();

    // Tell cargo to rerun this build script if any of the proto files change
    for proto_file in &proto_files {
        println!("cargo:rerun-if-changed={}", proto_file);
    }
} 