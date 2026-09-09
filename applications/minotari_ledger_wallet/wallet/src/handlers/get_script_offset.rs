// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
use minotari_ledger_wallet_common::script_offset::{
    ScriptOffsetHeaderError,
    check_script_key_count,
    check_sender_offset_key_count,
    parse_script_offset_header,
    sender_offset_index,
};
use tari_utilities::ByteArray;

use crate::{
    crypto::keys::RistrettoSecretKey,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes, get_random_u64},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_SPEND_INDEX,
};

pub struct ScriptOffsetCtx {
    sender_offset_sum: RistrettoSecretKey,
    script_private_key_sum: RistrettoSecretKey,
    account: u64,
    total_sender_offset_keys: u64,
    total_script_indexes: u64,
    total_derived_script_keys: u64,
}

// Implement constructor for TxInfo with default values
impl ScriptOffsetCtx {
    pub fn new() -> Self {
        Self {
            sender_offset_sum: RistrettoSecretKey::default(),
            script_private_key_sum: RistrettoSecretKey::default(),
            account: 0,
            total_sender_offset_keys: 0,
            total_script_indexes: 0,
            total_derived_script_keys: 0,
        }
    }

    /// Drop all accumulated state.
    ///
    /// The context outlives a single exchange, so anything that ends an exchange - a reply, an error, or an
    /// unrelated instruction - must call this. Otherwise a host whose chunk was rejected could resume the
    /// accumulation with a differently numbered follow-up chunk and read back a value the rejection was meant to
    /// withhold.
    pub fn reset(&mut self) {
        self.sender_offset_sum = RistrettoSecretKey::default();
        self.script_private_key_sum = RistrettoSecretKey::default();
        self.account = 0;
        self.total_sender_offset_keys = 0;
        self.total_script_indexes = 0;
        self.total_derived_script_keys = 0;
    }
}

fn header_error_to_app_sw(e: ScriptOffsetHeaderError) -> AppSW {
    match e {
        ScriptOffsetHeaderError::WrongLength | ScriptOffsetHeaderError::TooManySenderOffsetKeys => {
            AppSW::WrongApduLength
        },
        ScriptOffsetHeaderError::NoSenderOffsetKeys => AppSW::ScriptOffsetNoSenderOffsets,
        ScriptOffsetHeaderError::NoDeviceScriptKeys => AppSW::ScriptOffsetNoDeviceScriptKeys,
    }
}

/// Commit a validated header to `offset_ctx`.
///
/// Nothing is written before the header has passed every check: the context outlives a single exchange, so a header
/// that were written and then rejected would leave the counts the host asked for in place for the next chunk to act
/// on. `parse_script_offset_header` only yields a header once it has accepted it, and it is unit tested in
/// `minotari_ledger_wallet_common`.
fn read_instructions(offset_ctx: &mut ScriptOffsetCtx, data: &[u8]) -> Result<(), AppSW> {
    let header = parse_script_offset_header(data).map_err(header_error_to_app_sw)?;

    offset_ctx.account = header.account;
    offset_ctx.total_sender_offset_keys = header.sender_offset_count;
    offset_ctx.total_script_indexes = header.script_index_count;
    offset_ctx.total_derived_script_keys = header.derived_script_key_count;

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

fn derive_key_from_alpha(account: u64, data: &[u8]) -> Result<RistrettoSecretKey, AppSW> {
    if data.len() != 32 {
        return Err(AppSW::WrongApduLength);
    }
    let alpha = derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?;
    let blinding_factor: RistrettoSecretKey = get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();

    alpha_hasher(alpha, blinding_factor)
}

/// Calculate a script offset.
///
/// The host supplies only the script side of the sum: a partial sum of the script private keys it already knows, the
/// indexes of any pre-mine script keys, and the blinding factors of alpha derived script keys. The device generates
/// every sender offset key itself and returns the base index they were derived from, so the host can never choose -
/// or learn - the keys that blind the reply.
///
/// Wire format:
/// - chunk 0: `account(8) | sender_offset_count(8) | script_index_count(8) | derived_script_key_count(8)`
/// - chunk 1: `partial_script_key_sum(32)`
/// - next `script_index_count` chunks: `branch(8) | index(8)`
/// - next `derived_script_key_count` chunks: `blinding_factor(32)`
///
/// Reply: `version(1) | script_offset(32) | base_index(8)`, where the keys are
/// `base_index..base_index + sender_offset_count` walked with [`sender_offset_index`].
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

    // 2. partial sum of the script private keys the host already knows
    if chunk_number == 1 {
        if data.len() != 32 {
            return Err(AppSW::WrongApduLength);
        }
        let partial_script_key_sum: RistrettoSecretKey =
            get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();
        offset_ctx.script_private_key_sum = partial_script_key_sum;

        return Ok(());
    }

    let payload_offset = 2u64;

    // 3. Indexed script keys. The counts are host supplied, so saturate rather than wrap: a wrapped bound would
    //    silently move which chunk numbers land in which section.
    let end_script_indexes = payload_offset.saturating_add(offset_ctx.total_script_indexes);
    if (payload_offset..end_script_indexes).contains(&(chunk_number as u64)) {
        let (branch, index) = extract_branch_and_index(data)?;
        // The pre-mine branch holds the only script keys the wallet addresses by index; everything else would let
        // the host name a key of its choosing and read it back out of the offset.
        if branch != KeyType::PreMine {
            return Err(AppSW::ScriptOffsetInvalidScriptBranch);
        }
        let script_key = derive_from_bip32_key(offset_ctx.account, index, branch)?;

        offset_ctx.script_private_key_sum = &offset_ctx.script_private_key_sum + script_key;
    }

    // 4. Alpha derived script keys
    let end_derived_script_keys = end_script_indexes.saturating_add(offset_ctx.total_derived_script_keys);
    if (end_script_indexes..end_derived_script_keys).contains(&(chunk_number as u64)) {
        let k = derive_key_from_alpha(offset_ctx.account, data)?;

        offset_ctx.script_private_key_sum = &offset_ctx.script_private_key_sum + k
    }

    if more {
        return Ok(());
    }

    // 5. Re-check both counts at the point the value would actually leave the device, rather than trusting counts
    //    that were validated in an earlier exchange. Neither sum may leave unblinded by a term the host cannot
    //    compute: no sender offset key means the reply is the plain script key sum (the spend key), and no device
    //    derived script key means it is a device generated sender offset private key.
    check_sender_offset_key_count(offset_ctx.total_sender_offset_keys).map_err(header_error_to_app_sw)?;
    check_script_key_count(offset_ctx.total_script_indexes, offset_ctx.total_derived_script_keys)
        .map_err(header_error_to_app_sw)?;

    // 6. Generate the sender offset keys. One random base index is drawn from the device RNG and the keys are
    //    derived from `base..base + count`, so the host cannot replay a call and difference two replies to strip
    //    the blinding, and no per-key state has to be accumulated on the device.
    let base_index = get_random_u64();
    for i in 0..offset_ctx.total_sender_offset_keys {
        let index = sender_offset_index(base_index, i);
        let sender_offset = derive_from_bip32_key(offset_ctx.account, index, KeyType::OneSidedSenderOffset)?;

        offset_ctx.sender_offset_sum = &offset_ctx.sender_offset_sum + sender_offset;
    }

    let script_offset = &offset_ctx.script_private_key_sum - &offset_ctx.sender_offset_sum;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_offset.to_vec());
    comm.append(&base_index.to_le_bytes());
    offset_ctx.reset();

    Ok(())
}
