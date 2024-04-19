// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io;

use crate::AppSW;

pub fn handler_get_version(comm: &mut io::Comm) -> Result<(), AppSW> {
    let version = env!("CARGO_PKG_VERSION").as_bytes();
    comm.append(version);
    Ok(())
}
