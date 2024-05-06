// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::{io::Comm, ui::gadgets::SingleMessage};
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey, tari_utilities::ByteArray};

use crate::{
    utils::{derive_from_bip32_key, u64_to_string},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
};

pub fn handler_get_public_key(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let mut index_bytes = [0u8; 8];
    index_bytes.clone_from_slice(&data[8..16]);
    let index = u64::from_le_bytes(index_bytes);

    let mut key_bytes = [0u8; 8];
    key_bytes.clone_from_slice(&data[16..24]);
    let key_int = u64::from_le_bytes(key_bytes);
    let key = KeyType::from_branch_key(key_int);
    let second_key = KeyType::from_branch_key(key_int);

    let what_key = u64_to_string(second_key.as_byte() as u64);
    SingleMessage::new(&what_key).show_and_wait();

    let pk = match derive_from_bip32_key(account, index, key) {
        Ok(k) => RistrettoPublicKey::from_secret_key(&k),
        Err(e) => return Err(e),
    };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(pk.as_bytes());
    comm.reply_ok();

    Ok(())
}
