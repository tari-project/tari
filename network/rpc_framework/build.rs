//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

fn main() -> Result<(), Box<dyn std::error::Error>> {
    proto_builder::ProtobufCompiler::new()
        .proto_paths(&["proto"])
        .include_paths(&["proto"])
        .emit_rerun_if_changed_directives()
        .compile()?;

    Ok(())
}
