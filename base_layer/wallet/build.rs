// Copyright 2022. The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use taiji_common::build::StaticApplicationInfo;

fn main() {
    // generate version info
    let gen = StaticApplicationInfo::initialize().unwrap();
    gen.write_consts_to_outdir("consts.rs").unwrap();
}
