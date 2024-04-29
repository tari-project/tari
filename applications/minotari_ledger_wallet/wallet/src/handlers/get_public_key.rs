// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey, tari_utilities::ByteArray};

use crate::{utils::derive_from_bip32_key, AppSW, KeyType, RESPONSE_VERSION, STATIC_ALPHA_INDEX};

pub fn handler_get_public_key(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let pk = match derive_from_bip32_key(account, STATIC_ALPHA_INDEX, KeyType::Alpha) {
        Ok(k) => RistrettoPublicKey::from_secret_key(&k),
        Err(e) => return Err(e),
    };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(pk.as_bytes());
    comm.reply_ok();

    Ok(())
}
