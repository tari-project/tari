// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::vec::Vec;
use core::ops::Deref;

use ledger_device_sdk::io::Comm;
use tari_crypto::{ristretto::RistrettoSecretKey, tari_utilities::ByteArray};
use zeroize::Zeroizing;

use crate::{
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_SPEND_INDEX,
};

const MIN_UNIQUE_KEYS: usize = 2;

pub struct ScriptOffsetCtx {
    total_sender_offset_private_key: Zeroizing<RistrettoSecretKey>,
    total_script_private_key: Zeroizing<RistrettoSecretKey>,
    account: u64,
    total_offset_indexes: u64,
    total_commitment_keys: u64,
    unique_keys: Vec<Zeroizing<RistrettoSecretKey>>,
}

// Implement constructor for TxInfo with default values
impl ScriptOffsetCtx {
    pub fn new() -> Self {
        Self {
            total_sender_offset_private_key: Zeroizing::new(RistrettoSecretKey::default()),
            total_script_private_key: Zeroizing::new(RistrettoSecretKey::default()),
            account: 0,
            total_offset_indexes: 0,
            total_commitment_keys: 0,
            unique_keys: Vec::new(),
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.total_sender_offset_private_key = Zeroizing::new(RistrettoSecretKey::default());
        self.total_script_private_key = Zeroizing::new(RistrettoSecretKey::default());
        self.account = 0;
        self.total_offset_indexes = 0;
        self.total_commitment_keys = 0;
        self.unique_keys = Vec::new();
    }

    fn add_unique_key(&mut self, secret_key: Zeroizing<RistrettoSecretKey>) {
        if !self.unique_keys.contains(&secret_key) {
            self.unique_keys.push(secret_key);
        }
    }
}

fn read_instructions(offset_ctx: &mut ScriptOffsetCtx, data: &[u8]) {
    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    offset_ctx.account = u64::from_le_bytes(account_bytes);

    if data.len() < 16 {
        offset_ctx.total_offset_indexes = 0;
    } else {
        let mut total_offset_keys = [0u8; 8];
        total_offset_keys.clone_from_slice(&data[8..16]);
        offset_ctx.total_offset_indexes = u64::from_le_bytes(total_offset_keys);
    }

    if data.len() < 24 {
        offset_ctx.total_commitment_keys = 0;
    } else {
        let mut total_commitment_keys = [0u8; 8];
        total_commitment_keys.clone_from_slice(&data[16..24]);
        offset_ctx.total_commitment_keys = u64::from_le_bytes(total_commitment_keys);
    }
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
        return Ok(());
    }

    if chunk == 1 {
        // The sum of managed private keys
        let k: Zeroizing<RistrettoSecretKey> = get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();
        offset_ctx.total_script_private_key = Zeroizing::new(offset_ctx.total_script_private_key.deref() + k.deref());

        return Ok(());
    }

    let payload_offset = 2;
    let end_offset_indexes = payload_offset + offset_ctx.total_offset_indexes;

    if (payload_offset..end_offset_indexes).contains(&(chunk as u64)) {
        let mut index_bytes = [0u8; 8];
        index_bytes.clone_from_slice(&data[0..8]);
        let index = u64::from_le_bytes(index_bytes);

        let offset = derive_from_bip32_key(offset_ctx.account, index, KeyType::OneSidedSenderOffset)?;
        offset_ctx.add_unique_key(offset.clone());
        offset_ctx.total_sender_offset_private_key =
            Zeroizing::new(offset_ctx.total_sender_offset_private_key.deref() + offset.deref());
    }

    let end_commitment_keys = end_offset_indexes + offset_ctx.total_commitment_keys;

    if (end_offset_indexes..end_commitment_keys).contains(&(chunk as u64)) {
        let alpha = derive_from_bip32_key(offset_ctx.account, STATIC_SPEND_INDEX, KeyType::Spend)?;
        let blinding_factor: Zeroizing<RistrettoSecretKey> =
            get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();

        let k = alpha_hasher(alpha, blinding_factor)?;

        offset_ctx.add_unique_key(k.clone());
        offset_ctx.total_script_private_key = Zeroizing::new(offset_ctx.total_script_private_key.deref() + k.deref());
    }

    if more {
        return Ok(());
    }

    if offset_ctx.unique_keys.len() < MIN_UNIQUE_KEYS {
        return Err(AppSW::ScriptOffsetNotUnique);
    }

    let script_offset = Zeroizing::new(
        offset_ctx.total_script_private_key.deref() - offset_ctx.total_sender_offset_private_key.deref(),
    );

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_offset.to_vec());
    offset_ctx.reset();
    comm.reply_ok();

    Ok(())
}
