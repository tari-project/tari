// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::string::ToString;

use ledger_device_sdk::{ecc::make_bip32_path, io::Comm, ui::gadgets::SingleMessage};
use tari_crypto::{ristretto::RistrettoSecretKey, tari_utilities::ByteArray};

use crate::{
    utils::{get_raw_key, u64_to_string},
    AppSW,
};

static MINOTARI_LEDGER_ID: u32 = 535348;
static MINOTARI_ACCOUNT_ID: u32 = 7041;

pub fn handler_get_public_key(comm: &mut Comm, _display: bool) -> Result<(), AppSW> {
    // first 5 bytes are instruction details
    let offset = 5;
    let mut address_index_bytes = [0u8; 8];
    address_index_bytes.clone_from_slice(comm.get(offset, offset + 8));
    let address_index = u64_to_string(u64::from_le_bytes(address_index_bytes));

    let mut msg = "GetPrivateKey... ".to_string();
    msg.push_str(&address_index);
    SingleMessage::new(&msg).show();

    let mut bip32_path = "m/44'/".to_string();
    bip32_path.push_str(&MINOTARI_LEDGER_ID.to_string());
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&MINOTARI_ACCOUNT_ID.to_string());
    bip32_path.push_str(&"'/0/");
    bip32_path.push_str(&address_index);
    let path: [u32; 5] = make_bip32_path(bip32_path.as_bytes());

    let raw_key = get_raw_key(&path).map_err(|_| AppSW::KeyDeriveFail)?;

    let k = match RistrettoSecretKey::from_canonical_bytes(&raw_key) {
        Ok(val) => val,
        Err(_) => {
            SingleMessage::new("Err: key conversion").show();
            return Err(AppSW::KeyDeriveFail);
        },
    };
    comm.append(&[1]); // version
    comm.append(k.as_bytes());
    comm.reply_ok();

    Ok(())
}
