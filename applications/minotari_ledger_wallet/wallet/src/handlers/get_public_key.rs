// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::gadgets::SingleMessage;
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey, tari_utilities::ByteArray};

use crate::{utils::derive_from_bip32_key, AppSW, KeyType, RESPONSE_VERSION};

pub fn handler_get_public_key(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    if data.len() != 24 {
        SingleMessage::new("Invalid data length").show_and_wait();
        return Err(AppSW::WrongApduLength);
    }

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let mut index_bytes = [0u8; 8];
    index_bytes.clone_from_slice(&data[8..16]);
    let index = u64::from_le_bytes(index_bytes);

    let mut key_bytes = [0u8; 8];
    key_bytes.clone_from_slice(&data[16..24]);
    let key_int = u64::from_le_bytes(key_bytes);
    let key = KeyType::from_branch_key(key_int)?;

    let pk = match derive_from_bip32_key(account, index, key) {
        Ok(k) => RistrettoPublicKey::from_secret_key(&k),
        Err(e) => return Err(e),
    };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(pk.as_bytes());
    comm.reply_ok();

    Ok(())
}
