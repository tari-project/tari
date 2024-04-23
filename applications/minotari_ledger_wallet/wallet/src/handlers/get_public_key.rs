// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

use ledger_device_sdk::{ecc::make_bip32_path, io::Comm, ui::gadgets::SingleMessage};
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey, tari_utilities::ByteArray};

use crate::{
    alloc::string::ToString,
    utils::{get_key_from_uniform_bytes, get_raw_key, u64_to_string},
    AppSW,
    BIP32_COIN_TYPE,
    RESPONSE_VERSION,
};

const STATIC_INDEX: &str = "42";

pub fn handler_get_public_key(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64_to_string(u64::from_le_bytes(account_bytes));

    let mut bip32_path = "m/44'/".to_string();
    bip32_path.push_str(&BIP32_COIN_TYPE.to_string());
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&account);
    bip32_path.push_str(&"'/0/");
    bip32_path.push_str(STATIC_INDEX);
    let path: [u32; 5] = make_bip32_path(bip32_path.as_bytes());

    let raw_key = match get_raw_key(&path) {
        Ok(val) => val,
        Err(e) => {
            SingleMessage::new(&format!("Key error {:?}", e)).show_and_wait();
            return Err(AppSW::KeyDeriveFail);
        },
    };

    let pk = match get_key_from_uniform_bytes(&raw_key.as_ref()) {
        Ok(k) => RistrettoPublicKey::from_secret_key(&k),
        Err(e) => return Err(e),
    };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(pk.as_bytes());
    comm.reply_ok();

    Ok(())
}
