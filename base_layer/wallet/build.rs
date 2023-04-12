// Copyright 2022. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common::build::StaticApplicationInfo;

fn main() {
    // generate version info
    let gen = StaticApplicationInfo::initialize().unwrap();
    gen.write_consts_to_outdir("consts.rs").unwrap();
}
