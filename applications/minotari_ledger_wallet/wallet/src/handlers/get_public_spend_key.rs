// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
use minotari_ledger_wallet_common::codec::{Decode, GetPublicSpendKeyRequest, KeyReply};

use crate::{
    crypto::keys::RistrettoPublicKey,
    utils::derive_from_bip32_key,
    wire::{invalid_data_length, reply},
    AppSW,
    KeyType,
    STATIC_SPEND_INDEX,
};

pub fn handler_get_public_spend_key(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetPublicSpendKeyRequest::decode(data).map_err(|_| invalid_data_length())?;

    let pk = match derive_from_bip32_key(request.account, STATIC_SPEND_INDEX, KeyType::Spend) {
        Ok(k) => RistrettoPublicKey::from_secret_key(&k),
        Err(e) => return Err(e),
    };

    reply(comm, &KeyReply::new(pk.as_array()));

    Ok(())
}
