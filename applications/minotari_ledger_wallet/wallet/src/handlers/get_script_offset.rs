// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
use minotari_ledger_wallet_common::script_offset::{
    ScriptKeySection,
    ScriptOffsetHeaderError,
    check_offset_is_blinded,
    parse_script_offset_header,
    script_key_section,
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
    /// The sum of the script keys the *device* derived, from a pre-mine index or from a blinding factor folded into
    /// `alpha`. This is the only part of the script side that blinds the reply against the host.
    device_script_key_sum: RistrettoSecretKey,
    /// The one opaque scalar the host computed itself. Kept apart from `device_script_key_sum` so that the chunk
    /// carrying it cannot overwrite what the device derived: it arrives as a whole sum rather than a term, and the
    /// host chooses which chunk numbers to send and in which order.
    host_partial_script_key_sum: RistrettoSecretKey,
    /// How many script keys the device actually derived and added to `device_script_key_sum`.
    ///
    /// This is what the emission check looks at. The counts the header declared say only what the host *asked* for;
    /// a host is free to declare a section and then never send a chunk that falls inside it, so a check on the
    /// declared counts would pass while the script side of the sum was still zero.
    device_script_keys_folded: u64,
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
            device_script_key_sum: RistrettoSecretKey::default(),
            host_partial_script_key_sum: RistrettoSecretKey::default(),
            device_script_keys_folded: 0,
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
        self.device_script_key_sum = RistrettoSecretKey::default();
        self.host_partial_script_key_sum = RistrettoSecretKey::default();
        self.device_script_keys_folded = 0;
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
///
/// The host chooses which chunk numbers it sends, in which order, and which one terminates the exchange. Nothing
/// here may therefore depend on the host having followed the format: the counts in the header say what the host
/// asked for, and the only thing that decides whether the reply is safe to emit is what the device actually
/// derived. See [`check_offset_is_blinded`].
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

    // 2. Partial sum of the script private keys the host already knows. It arrives as a whole sum rather than a
    //    term, so it is stored on its own: assigning it into the running total would let a host fold a device
    //    derived key, wipe it with this chunk, and still satisfy a check that counted the folded key.
    if chunk_number == 1 {
        if data.len() != 32 {
            return Err(AppSW::WrongApduLength);
        }
        let partial_script_key_sum: RistrettoSecretKey =
            get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();
        offset_ctx.host_partial_script_key_sum = partial_script_key_sum;

        return Ok(());
    }

    // 3. Fold a script key, if this chunk carries one. Which section a chunk number belongs to is decided by
    //    `script_key_section`, which lives in `minotari_ledger_wallet_common` so that its boundaries - the ones the
    //    host picks its chunk numbers against - are unit tested without a device.
    let script_key = match script_key_section(
        u64::from(chunk_number),
        offset_ctx.total_script_indexes,
        offset_ctx.total_derived_script_keys,
    ) {
        ScriptKeySection::IndexedScriptKey => {
            let (branch, index) = extract_branch_and_index(data)?;
            // The pre-mine branch holds the only script keys the wallet addresses by index; everything else would
            // let the host name a key of its choosing and read it back out of the offset.
            if branch != KeyType::PreMine {
                return Err(AppSW::ScriptOffsetInvalidScriptBranch);
            }
            Some(derive_from_bip32_key(offset_ctx.account, index, branch)?)
        },
        ScriptKeySection::DerivedScriptKey => Some(derive_key_from_alpha(offset_ctx.account, data)?),
        // This chunk carries nothing that blinds the reply. A host is free to send one and then terminate, which
        // is why the counter below - not the header - is what the emission check looks at.
        ScriptKeySection::None => None,
    };
    if let Some(script_key) = script_key {
        offset_ctx.device_script_key_sum = &offset_ctx.device_script_key_sum + script_key;
        offset_ctx.device_script_keys_folded = offset_ctx.device_script_keys_folded.saturating_add(1);
    }

    if more {
        return Ok(());
    }

    // 4. Decide, at the point the value would actually leave, whether it is blinded on both sides. The script side
    //    is judged on what the device folded, never on what the header declared: the host picks the chunk numbers,
    //    so it can declare a section and then terminate on a chunk outside it, folding nothing.
    //
    //    `total_sender_offset_keys` is safe to take from the header because step 5 below derives exactly that many
    //    keys and sums every one of them, so declared and actual cannot diverge - and the header check already
    //    bounded it.
    check_offset_is_blinded(
        offset_ctx.total_sender_offset_keys,
        offset_ctx.device_script_keys_folded,
    )
    .map_err(header_error_to_app_sw)?;

    // 5. Generate the sender offset keys. One random base index is drawn from the device RNG and the keys are
    //    derived from `base..base + count`, so the host cannot replay a call and difference two replies to strip
    //    the blinding, and no per-key state has to be accumulated on the device.
    let base_index = get_random_u64();
    for i in 0..offset_ctx.total_sender_offset_keys {
        let index = sender_offset_index(base_index, i);
        let sender_offset = derive_from_bip32_key(offset_ctx.account, index, KeyType::OneSidedSenderOffset)?;

        offset_ctx.sender_offset_sum = &offset_ctx.sender_offset_sum + sender_offset;
    }

    let script_key_sum = &offset_ctx.device_script_key_sum + &offset_ctx.host_partial_script_key_sum;
    let script_offset = &script_key_sum - &offset_ctx.sender_offset_sum;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_offset.to_vec());
    comm.append(&base_index.to_le_bytes());
    offset_ctx.reset();

    Ok(())
}
