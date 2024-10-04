// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use core::ops::Deref;

use ledger_device_sdk::io::Comm;
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::NbglStatus;
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::gadgets::SingleMessage;
use tari_crypto::{ristretto::RistrettoPublicKey, tari_utilities::ByteArray};
use zeroize::Zeroizing;

use crate::{
    utils::{derive_from_bip32_key, get_key_from_canonical_bytes},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
};

pub fn handler_get_dh_shared_secret(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    if data.len() != 56 {
        #[cfg(not(any(target_os = "stax", target_os = "flex")))]
        {
            SingleMessage::new("Invalid data length").show_and_wait();
        }
        #[cfg(any(target_os = "stax", target_os = "flex"))]
        {
            NbglStatus::new().text(&"Invalid data length").show(false);
        }

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

    let public_key: RistrettoPublicKey = get_key_from_canonical_bytes(&data[24..56])?;

    let shared_secret_key = match derive_from_bip32_key(account, index, key) {
        Ok(k) => Zeroizing::new(k * public_key),
        Err(e) => return Err(e),
    };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(shared_secret_key.deref().as_bytes());
    comm.reply_ok();

    Ok(())
}
