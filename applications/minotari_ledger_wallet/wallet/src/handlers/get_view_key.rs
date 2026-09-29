// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
use minotari_ledger_wallet_common::codec::{Decode, GetViewKeyRequest, KeyReply};

use crate::{
    utils::derive_from_bip32_key,
    wire::{invalid_data_length, reply},
    AppSW,
    KeyType,
    STATIC_VIEW_INDEX,
};

pub fn handler_get_view_key(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetViewKeyRequest::decode(data).map_err(|_| invalid_data_length())?;

    let p = match derive_from_bip32_key(request.account, STATIC_VIEW_INDEX, KeyType::ViewKey) {
        Ok(k) => k,
        Err(e) => return Err(e),
    };

    reply(comm, &KeyReply::new(p.as_array()));

    Ok(())
}
