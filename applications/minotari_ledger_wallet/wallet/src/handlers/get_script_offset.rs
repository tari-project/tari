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
    sender_offset_sum: Zeroizing<RistrettoSecretKey>,
    script_private_key_sum: Zeroizing<RistrettoSecretKey>,
    account: u64,
    total_offset_indexes: u64,
    total_script_indexes: u64,
    total_derived_offset_keys: u64,
    total_derived_script_keys: u64,
    unique_keys: Vec<Zeroizing<RistrettoSecretKey>>,
}

// Implement constructor for TxInfo with default values
impl ScriptOffsetCtx {
    pub fn new() -> Self {
        Self {
            sender_offset_sum: Zeroizing::new(RistrettoSecretKey::default()),
            script_private_key_sum: Zeroizing::new(RistrettoSecretKey::default()),
            account: 0,
            total_offset_indexes: 0,
            total_script_indexes: 0,
            total_derived_offset_keys: 0,
            total_derived_script_keys: 0,
            unique_keys: Vec::new(),
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.sender_offset_sum = Zeroizing::new(RistrettoSecretKey::default());
        self.script_private_key_sum = Zeroizing::new(RistrettoSecretKey::default());
        self.account = 0;
        self.total_offset_indexes = 0;
        self.total_script_indexes = 0;
        self.total_derived_offset_keys = 0;
        self.total_derived_script_keys = 0;
        self.unique_keys = Vec::new();
    }

    fn add_unique_key(&mut self, secret_key: Zeroizing<RistrettoSecretKey>) {
        if !self.unique_keys.contains(&secret_key) {
            self.unique_keys.push(secret_key);
        }
    }
}

fn read_instructions(offset_ctx: &mut ScriptOffsetCtx, data: &[u8]) -> Result<(), AppSW> {
    if data.len() != 40 {
        return Err(AppSW::WrongApduLength);
    }

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    offset_ctx.account = u64::from_le_bytes(account_bytes);

    let mut total_offset_keys = [0u8; 8];
    total_offset_keys.clone_from_slice(&data[8..16]);
    offset_ctx.total_offset_indexes = u64::from_le_bytes(total_offset_keys);

    let mut total_script_indexes = [0u8; 8];
    total_script_indexes.clone_from_slice(&data[16..24]);
    offset_ctx.total_script_indexes = u64::from_le_bytes(total_script_indexes);

    let mut total_derived_offset_keys = [0u8; 8];
    total_derived_offset_keys.clone_from_slice(&data[24..32]);
    offset_ctx.total_derived_offset_keys = u64::from_le_bytes(total_derived_offset_keys);

    let mut total_derived_script_keys = [0u8; 8];
    total_derived_script_keys.clone_from_slice(&data[32..40]);
    offset_ctx.total_derived_script_keys = u64::from_le_bytes(total_derived_script_keys);

    Ok(())
}

fn extract_branch_and_index(data: &[u8]) -> Result<(KeyType, u64), AppSW> {
    if data.len() != 16 {
        return Err(AppSW::WrongApduLength);
    }
    let mut branch_bytes = [0u8; 8];
    branch_bytes.clone_from_slice(&data[0..8]);
    let branch_int = u64::from_le_bytes(branch_bytes);
    let branch = KeyType::from_branch_key(branch_int)?;

    let mut index_bytes = [0u8; 8];
    index_bytes.clone_from_slice(&data[8..16]);
    let index = u64::from_le_bytes(index_bytes);

    Ok((branch, index))
}

fn derive_key_from_alpha(account: u64, data: &[u8]) -> Result<Zeroizing<RistrettoSecretKey>, AppSW> {
    if data.len() != 32 {
        return Err(AppSW::WrongApduLength);
    }
    let alpha = derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?;
    let blinding_factor: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();

    alpha_hasher(alpha, blinding_factor)
}

pub fn handler_get_script_offset(
    comm: &mut Comm,
    chunk_number: u8,
    more: bool,
    offset_ctx: &mut ScriptOffsetCtx,
) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    // 1. data sizes
    if chunk_number == 0 {
        // Reset offset context
        offset_ctx.reset();
        read_instructions(offset_ctx, data)?;
        return Ok(());
    }

    // 2. partial_script_offset
    if chunk_number == 1 {
        // Initialize 'script_private_key_sum' with 'partial_script_offset'
        let partial_script_offset: Zeroizing<RistrettoSecretKey> =
            get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();
        offset_ctx.script_private_key_sum = partial_script_offset;

        return Ok(());
    }

    let payload_offset = 2;
    let end_offset_indexes = payload_offset + offset_ctx.total_offset_indexes;

    // 3. Indexed Sender offset
    if (payload_offset..end_offset_indexes).contains(&(chunk_number as u64)) {
        let (branch, index) = extract_branch_and_index(data)?;
        let offset = derive_from_bip32_key(offset_ctx.account, index, branch)?;

        offset_ctx.add_unique_key(offset.clone());
        offset_ctx.sender_offset_sum = Zeroizing::new(offset_ctx.sender_offset_sum.deref() + offset.deref());
    }

    // 4. Indexed Script key
    let end_script_indexes = end_offset_indexes + offset_ctx.total_script_indexes;
    if (end_offset_indexes..end_script_indexes).contains(&(chunk_number as u64)) {
        let (branch, index) = extract_branch_and_index(data)?;
        let script_key = derive_from_bip32_key(offset_ctx.account, index, branch)?;

        offset_ctx.add_unique_key(script_key.clone());
        offset_ctx.script_private_key_sum =
            Zeroizing::new(offset_ctx.script_private_key_sum.deref() + script_key.deref());
    }

    // 5. Derived sender offsets key
    let end_derived_offset_keys = end_script_indexes + offset_ctx.total_derived_offset_keys;
    if (end_script_indexes..end_derived_offset_keys).contains(&(chunk_number as u64)) {
        let k = derive_key_from_alpha(offset_ctx.account, data)?;

        offset_ctx.add_unique_key(k.clone());
        offset_ctx.sender_offset_sum = Zeroizing::new(offset_ctx.sender_offset_sum.deref() + k.deref());
    }

    // 6. Derived script key
    let end_derived_script_keys = end_derived_offset_keys + offset_ctx.total_derived_script_keys;
    if (end_derived_offset_keys..end_derived_script_keys).contains(&(chunk_number as u64)) {
        let k = derive_key_from_alpha(offset_ctx.account, data)?;

        offset_ctx.add_unique_key(k.clone());
        offset_ctx.script_private_key_sum = Zeroizing::new(offset_ctx.script_private_key_sum.deref() + k.deref());
    }

    if more {
        return Ok(());
    }

    // Guard against attacks to extract the spending private key
    if offset_ctx.unique_keys.len() < MIN_UNIQUE_KEYS {
        return Err(AppSW::ScriptOffsetNotUnique);
    }

    let script_offset =
        Zeroizing::new(offset_ctx.script_private_key_sum.deref() - offset_ctx.sender_offset_sum.deref());

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_offset.to_vec());
    offset_ctx.reset();
    comm.reply_ok();

    Ok(())
}
