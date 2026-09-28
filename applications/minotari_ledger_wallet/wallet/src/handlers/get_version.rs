// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io;
use minotari_ledger_wallet_common::codec::TextReply;

use crate::{wire::reply, AppSW};

pub fn handler_get_version(comm: &mut io::Comm) -> Result<(), AppSW> {
    reply(
        comm,
        &TextReply {
            text: env!("CARGO_PKG_VERSION").as_bytes(),
        },
    );
    Ok(())
}
