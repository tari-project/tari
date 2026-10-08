// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
use minotari_ledger_wallet_common::{
    codec::{Decode, DerivedScriptKeyChunk, PartialScriptKeySumChunk, ScriptKeyIndexChunk, ScriptOffsetReply},
    script_offset::{
        ScriptKeySection,
        ScriptOffsetHeaderError,
        check_indexed_script_key_index,
        check_offset_is_blinded,
        parse_script_offset_header,
        script_key_section,
        sender_offset_base_index,
        sender_offset_branch,
        sender_offset_index,
    },
};

use crate::{
    crypto::keys::RistrettoSecretKey,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes, get_random_u64},
    wire::reply,
    AppSW,
    KeyType,
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
    /// How many of those were alpha derived. None means every script key the device derived was a pre-mine key,
    /// which is what puts the sender offset keys on the `PreMine` branch - see `sender_offset_branch`.
    derived_script_keys_folded: u64,
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
            derived_script_keys_folded: 0,
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
        self.derived_script_keys_folded = 0;
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
        // A sender offset index is not a pre-mine script key, so it gets the same status word as any other script
        // key the device will not fold.
        ScriptOffsetHeaderError::SenderOffsetIndexAsScriptKey => AppSW::ScriptOffsetInvalidScriptBranch,
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
    let chunk = ScriptKeyIndexChunk::decode(data).map_err(|_| AppSW::WrongApduLength)?;
    let branch = KeyType::from_branch_key(chunk.branch)?;

    Ok((branch, chunk.index))
}

fn derive_key_from_alpha(account: u64, data: &[u8]) -> Result<RistrettoSecretKey, AppSW> {
    let chunk = DerivedScriptKeyChunk::decode(data).map_err(|_| AppSW::WrongApduLength)?;
    // `alpha` is derived before the blinding factor's canonical check, as it always was: the order decides which
    // status word a malformed chunk gets.
    let alpha = derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?;
    let blinding_factor: RistrettoSecretKey =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(chunk.blinding_factor)?.into();

    alpha_hasher(alpha, blinding_factor)
}

/// Calculate a script offset.
///
/// The host supplies only the script side of the sum: a partial sum of the script private keys it already knows, the
/// indexes of any pre-mine script keys, and the blinding factors of alpha derived script keys. The device generates
/// every sender offset key itself and returns the base index they were derived from, so the host can never choose -
/// or learn - the keys that blind the reply.
///
/// Wire format: see `minotari_ledger_wallet_common::codec::script_offset`, which defines every chunk and the reply.
/// Which chunk type a chunk number is decoded as is decided here, by [`script_key_section`], never by the codec. The
/// reply names the sender offset keys as `base_index..base_index + sender_offset_count`, walked with
/// [`sender_offset_index`].
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
        let chunk = PartialScriptKeySumChunk::decode(data).map_err(|_| AppSW::WrongApduLength)?;
        let partial_script_key_sum: RistrettoSecretKey =
            get_key_from_canonical_bytes::<RistrettoSecretKey>(chunk.partial_script_key_sum)?.into();
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
            // ...and of those, only the genesis script keys: an index in the pre-mine sender offset range names a key
            // this instruction issued as a blinding term, and folding it back in as a script term would let replies
            // be chained and telescoped. See `check_indexed_script_key_index`.
            check_indexed_script_key_index(index).map_err(header_error_to_app_sw)?;
            Some(derive_from_bip32_key(offset_ctx.account, index, branch)?)
        },
        ScriptKeySection::DerivedScriptKey => {
            let script_key = derive_key_from_alpha(offset_ctx.account, data)?;
            offset_ctx.derived_script_keys_folded = offset_ctx.derived_script_keys_folded.saturating_add(1);
            Some(script_key)
        },
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
    //    derived from `base..base + count`, so no per-key state has to be accumulated on the device. The reply
    //    hands the host the base, and two replies whose bases name the same keys can be differenced to strip the
    //    blinding - so what stops that is that bases do not repeat: 64 random bits on `OneSidedSenderOffset`, 62 on
    //    `PreMine`, every one of them derived from (see `derive_from_bip32_key` and `sender_offset_base_index`).
    //
    //    The branch is decided by what was folded, not by what the header declared. With no alpha derived key in
    //    the sum this is the pre-mine spend flow, and the keys go on `PreMine` - the one branch the legacy nonce
    //    instruction signs, so that pre-mine step 3 can sign its metadata signature with them. Any reply that folds
    //    `alpha` stays blinded by `OneSidedSenderOffset` keys, which the legacy instruction refuses; otherwise two
    //    legacy signatures would give up `k_i`, and `alpha` with it. See `minotari_ledger_wallet_common::legacy_nonce`.
    let branch = sender_offset_branch(offset_ctx.derived_script_keys_folded);
    let key_type = KeyType::from_branch_key(u64::from(branch.as_byte()))?;
    let base_index = sender_offset_base_index(branch, get_random_u64());
    for i in 0..offset_ctx.total_sender_offset_keys {
        let index = sender_offset_index(base_index, i);
        let sender_offset = derive_from_bip32_key(offset_ctx.account, index, key_type)?;

        offset_ctx.sender_offset_sum = &offset_ctx.sender_offset_sum + sender_offset;
    }

    let script_key_sum = &offset_ctx.device_script_key_sum + &offset_ctx.host_partial_script_key_sum;
    let script_offset = &script_key_sum - &offset_ctx.sender_offset_sum;

    reply(comm, &ScriptOffsetReply::new(script_offset.as_array(), base_index));
    offset_ctx.reset();

    Ok(())
}
