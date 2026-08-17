// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
use minotari_ledger_wallet_common::{
    common_types::AppSW as AppSWMapping,
    script_offset_policy::{
        ChunkKind,
        ScriptOffsetPolicyError,
        ScriptOffsetRequestGuard,
        ScriptOffsetRole,
        ScriptOffsetTotals,
    },
};
use tari_utilities::ByteArray;

use crate::{
    crypto::keys::RistrettoSecretKey,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes},
    AppSW,
    KeyType,
    MAX_PAYLOADS,
    RESPONSE_VERSION,
    STATIC_SPEND_INDEX,
};

/// Which keys a request may reference, and whether it is complete, is decided by
/// [`minotari_ledger_wallet_common::script_offset_policy`]; this handler only derives and sums. Keeping the two apart
/// means the rules protecting the spend key are exercised by ordinary unit tests rather than only on a device.
pub struct ScriptOffsetCtx {
    sender_offset_sum: RistrettoSecretKey,
    script_private_key_sum: RistrettoSecretKey,
    account: u64,
    guard: ScriptOffsetRequestGuard,
}

// Implement constructor for TxInfo with default values
impl ScriptOffsetCtx {
    pub fn new() -> Self {
        Self {
            sender_offset_sum: RistrettoSecretKey::default(),
            script_private_key_sum: RistrettoSecretKey::default(),
            account: 0,
            guard: ScriptOffsetRequestGuard::new(),
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.sender_offset_sum = RistrettoSecretKey::default();
        self.script_private_key_sum = RistrettoSecretKey::default();
        self.account = 0;
        self.guard.reset();
    }
}

/// Abandon the request and report the violation. Nothing accumulated so far may survive into the next request.
fn reject(offset_ctx: &mut ScriptOffsetCtx, e: ScriptOffsetPolicyError) -> AppSW {
    let sw = e.app_sw();
    offset_ctx.reset();
    match sw {
        AppSWMapping::BadBranchKey => AppSW::BadBranchKey,
        AppSWMapping::ScriptOffsetNotUnique => AppSW::ScriptOffsetNotUnique,
        _ => AppSW::WrongApduLength,
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

    let mut total_script_indexes = [0u8; 8];
    total_script_indexes.clone_from_slice(&data[16..24]);

    let mut total_derived_offset_keys = [0u8; 8];
    total_derived_offset_keys.clone_from_slice(&data[24..32]);

    let mut total_derived_script_keys = [0u8; 8];
    total_derived_script_keys.clone_from_slice(&data[32..40]);

    let totals = ScriptOffsetTotals {
        sender_offset_indexes: u64::from_le_bytes(total_offset_keys),
        script_key_indexes: u64::from_le_bytes(total_script_indexes),
        derived_sender_offsets: u64::from_le_bytes(total_derived_offset_keys),
        derived_script_keys: u64::from_le_bytes(total_derived_script_keys),
    };
    if let Err(e) = offset_ctx.guard.begin(totals, u64::from(MAX_PAYLOADS)) {
        return Err(reject(offset_ctx, e));
    }

    Ok(())
}

fn extract_branch_and_index(data: &[u8]) -> Result<(u8, u64), AppSW> {
    if data.len() != 16 {
        return Err(AppSW::WrongApduLength);
    }
    let mut branch_bytes = [0u8; 8];
    branch_bytes.clone_from_slice(&data[0..8]);
    let branch = u8::try_from(u64::from_le_bytes(branch_bytes)).map_err(|_| AppSW::BadBranchKey)?;

    let mut index_bytes = [0u8; 8];
    index_bytes.clone_from_slice(&data[8..16]);
    let index = u64::from_le_bytes(index_bytes);

    Ok((branch, index))
}

/// Derive an indexed key, having first established that the host is allowed to reference it in this role.
fn indexed_key(
    offset_ctx: &mut ScriptOffsetCtx,
    data: &[u8],
    role: ScriptOffsetRole,
) -> Result<RistrettoSecretKey, AppSW> {
    let (branch_byte, index) = extract_branch_and_index(data)?;
    let branch = match offset_ctx.guard.record_indexed_key(branch_byte, index, role) {
        Ok(branch) => branch,
        Err(e) => return Err(reject(offset_ctx, e)),
    };

    derive_from_bip32_key(offset_ctx.account, index, KeyType::from_branch(branch))
}

fn derive_key_from_alpha(
    account: u64,
    data: &[u8],
    offset_ctx: &mut ScriptOffsetCtx,
) -> Result<RistrettoSecretKey, AppSW> {
    if data.len() != 32 {
        return Err(AppSW::WrongApduLength);
    }
    let alpha = derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?;
    let blinding_factor: RistrettoSecretKey = get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();

    offset_ctx.guard.record_alpha();

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

    let chunk = u64::from(chunk_number);
    let kind = match offset_ctx.guard.classify_chunk(chunk) {
        Ok(kind) => kind,
        Err(e) => return Err(reject(offset_ctx, e)),
    };

    match kind {
        // 2. partial_script_offset
        ChunkKind::PartialOffset => {
            // Initialize 'script_private_key_sum' with 'partial_script_offset'
            let partial_script_offset: RistrettoSecretKey =
                get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();
            offset_ctx.script_private_key_sum = partial_script_offset;

            return Ok(());
        },

        // 3. Indexed Sender offset
        ChunkKind::SenderOffsetIndex => {
            let offset = indexed_key(offset_ctx, data, ScriptOffsetRole::SenderOffset)?;
            offset_ctx.sender_offset_sum = &offset_ctx.sender_offset_sum + offset;
        },

        // 4. Indexed Script key
        ChunkKind::ScriptKeyIndex => {
            let script_key = indexed_key(offset_ctx, data, ScriptOffsetRole::ScriptKey)?;
            offset_ctx.script_private_key_sum = &offset_ctx.script_private_key_sum + script_key;
        },

        // 5. Derived sender offsets key
        ChunkKind::DerivedSenderOffset => {
            let k = derive_key_from_alpha(offset_ctx.account, data, offset_ctx)?;
            offset_ctx.sender_offset_sum = &offset_ctx.sender_offset_sum + k;
        },

        // 6. Derived script key
        ChunkKind::DerivedScriptKey => {
            let k = derive_key_from_alpha(offset_ctx.account, data, offset_ctx)?;
            offset_ctx.script_private_key_sum = &offset_ctx.script_private_key_sum + k;
        },
    }

    if more {
        return Ok(());
    }

    // Guard against attacks to extract the spending private key: the request must be complete, and the offset must
    // be a function of more than one Ledger key.
    if let Err(e) = offset_ctx.guard.finish(chunk) {
        return Err(reject(offset_ctx, e));
    }

    let script_offset = &offset_ctx.script_private_key_sum - &offset_ctx.sender_offset_sum;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_offset.to_vec());
    offset_ctx.reset();

    Ok(())
}
