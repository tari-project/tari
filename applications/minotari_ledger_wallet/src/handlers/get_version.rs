// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use core::str::FromStr;

use ledger_device_sdk::io;

use crate::AppSW;

pub fn handler_get_version(comm: &mut io::Comm) -> Result<(), AppSW> {
    if let Some((major, minor, patch)) = parse_version_string(env!("CARGO_PKG_VERSION")) {
        comm.append(&[major, minor, patch]);
        Ok(())
    } else {
        Err(AppSW::VersionParsingFail)
    }
}

fn parse_version_string(input: &str) -> Option<(u8, u8, u8)> {
    // Split the input string by '.'.
    // Input should be of the form "major.minor.patch",
    // where "major", "minor", and "patch" are integers.
    let mut parts = input.split('.');
    let major = u8::from_str(parts.next()?).ok()?;
    let minor = u8::from_str(parts.next()?).ok()?;
    let patch = u8::from_str(parts.next()?).ok()?;
    Some((major, minor, patch))
}
