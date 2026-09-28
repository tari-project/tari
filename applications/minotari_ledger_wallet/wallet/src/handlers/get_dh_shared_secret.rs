// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use core::ops::Deref;

use ledger_device_sdk::io::Comm;
use minotari_ledger_wallet_common::codec::{Decode, GetDHSharedSecretRequest, KeyReply};
use zeroize::Zeroizing;

use crate::{
    crypto::keys::RistrettoPublicKey,
    utils::{derive_from_bip32_key, get_key_from_canonical_bytes},
    wire::{invalid_data_length, reply},
    AppSW,
    KeyType,
};

pub fn handler_get_dh_shared_secret(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetDHSharedSecretRequest::decode(data).map_err(|_| invalid_data_length())?;

    let key = KeyType::from_branch_key(request.branch)?;

    let public_key: RistrettoPublicKey = get_key_from_canonical_bytes(request.public_key)?;

    let shared_secret_key = match derive_from_bip32_key(request.account, request.index, key) {
        Ok(k) => Zeroizing::new(k * public_key),
        Err(e) => return Err(e),
    };

    reply(comm, &KeyReply::new(shared_secret_key.deref().as_array()));

    Ok(())
}
