// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common::build::StaticApplicationInfo;

fn main() {
    // generate version info
    let info = StaticApplicationInfo::initialize().unwrap();
    info.write_consts_to_outdir("consts.rs").unwrap();
}
