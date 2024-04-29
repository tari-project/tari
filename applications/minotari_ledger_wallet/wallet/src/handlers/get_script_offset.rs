// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::{io::Comm, ui::gadgets::SingleMessage};
use tari_crypto::{ristretto::RistrettoSecretKey, tari_utilities::ByteArray};

use crate::{
    alloc::string::ToString,
    utils::{derive_from_bip32_key, get_key_from_canonical_bytes, mask_a},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_ALPHA_INDEX,
};

pub struct ScriptOffsetCtx {
    total_sender_offset_private_key: RistrettoSecretKey,
    total_script_private_key: RistrettoSecretKey,
    account: u64,
    total_offset_indexes: u64,
    total_commitment_keys: u64,
}

// Implement constructor for TxInfo with default values
impl ScriptOffsetCtx {
    pub fn new() -> Self {
        Self {
            total_sender_offset_private_key: RistrettoSecretKey::default(),
            total_script_private_key: RistrettoSecretKey::default(),
            account: 0,
            total_offset_indexes: 0,
            total_commitment_keys: 0,
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.total_sender_offset_private_key = RistrettoSecretKey::default();
        self.total_script_private_key = RistrettoSecretKey::default();
        self.account = 0;
        self.total_offset_indexes = 0;
        self.total_commitment_keys = 0;
    }
}

fn read_instructions(offset_ctx: &mut ScriptOffsetCtx, data: &[u8]) {
    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    offset_ctx.account = u64::from_le_bytes(account_bytes);

    let mut total_offset_keys = [0u8; 8];
    total_offset_keys.clone_from_slice(&data[24..32]);
    offset_ctx.total_offset_indexes = u64::from_le_bytes(total_offset_keys);

    let mut total_commitment_keys = [0u8; 8];
    total_commitment_keys.clone_from_slice(&data[16..24]);
    offset_ctx.total_commitment_keys = u64::from_le_bytes(total_commitment_keys);
}
pub fn handler_get_script_offset(
    comm: &mut Comm,
    chunk: u8,
    more: bool,
    offset_ctx: &mut ScriptOffsetCtx,
) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    if chunk == 0 {
        // Reset offset context
        offset_ctx.reset();
        read_instructions(offset_ctx, data);
    }

    if chunk == 1 {
        // The sum of managed private keys
        let k = get_key_from_canonical_bytes(&data[0..32])?;
        offset_ctx.total_script_private_key = &offset_ctx.total_script_private_key + k;
    }

    let payload_offset = 2;
    let end_offset_indexes = payload_offset + offset_ctx.total_offset_indexes;
    if (payload_offset..end_offset_indexes).contains(&(chunk as u64)) {
        let mut index_bytes = [0u8; 8];
        index_bytes.clone_from_slice(&data[0..8]);
        let index = u64::from_le_bytes(index_bytes);

        let offset = derive_from_bip32_key(offset_ctx.account, index, KeyType::ScriptOffset)?;
        offset_ctx.total_offset_indexes = &offset_ctx.total_offset_indexes + &offset;
    }

    let end_commitment_keys = end_offset_indexes + offset_ctx.total_commitment_keys;
    if (end_offset_indexes..end_commitment_keys).contains(&(chunk as u64)) {
        let alpha = derive_from_bip32_key(offset_ctx.account, STATIC_ALPHA_INDEX, KeyType::Alpha)?;
        let commitment = get_key_from_canonical_bytes(&data[0..32])?;
        let k = mask_a(alpha, commitment)?;

        offset_ctx.total_script_private_key = &offset_ctx.total_script_private_key + &k;
    }

    if more {
        return Ok(());
    }

    let script_offset = &offset_ctx.total_script_private_key - &offset_ctx.total_sender_offset_private_key;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_offset.to_vec());
    comm.reply_ok();

    SingleMessage::new(&"Finished Offset!".to_string()).show_and_wait();

    offset_ctx.reset();
    Ok(())
}
