// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::vec::Vec;

use ledger_device_sdk::io::Comm;
use tari_utilities::ByteArray;

use crate::{
    crypto::keys::RistrettoSecretKey,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_SPEND_INDEX,
};

const MIN_UNIQUE_KEYS: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
struct KeyIdentity {
    account: u64,
    branch: KeyType,
    index: u64,
}

pub struct ScriptOffsetCtx {
    sender_offset_sum: RistrettoSecretKey,
    script_private_key_sum: RistrettoSecretKey,
    account: u64,
    total_offset_indexes: u64,
    total_script_indexes: u64,
    total_derived_offset_keys: u64,
    total_derived_script_keys: u64,
    sender_key_identities: Vec<KeyIdentity>,
    script_key_identities: Vec<KeyIdentity>,
}

// Implement constructor for TxInfo with default values
impl ScriptOffsetCtx {
    pub fn new() -> Self {
        Self {
            sender_offset_sum: RistrettoSecretKey::default(),
            script_private_key_sum: RistrettoSecretKey::default(),
            account: 0,
            total_offset_indexes: 0,
            total_script_indexes: 0,
            total_derived_offset_keys: 0,
            total_derived_script_keys: 0,
            sender_key_identities: Vec::new(),
            script_key_identities: Vec::new(),
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.sender_offset_sum = RistrettoSecretKey::default();
        self.script_private_key_sum = RistrettoSecretKey::default();
        self.account = 0;
        self.total_offset_indexes = 0;
        self.total_script_indexes = 0;
        self.total_derived_offset_keys = 0;
        self.total_derived_script_keys = 0;
        self.sender_key_identities = Vec::new();
        self.script_key_identities = Vec::new();
    }

    fn add_sender_identity(&mut self, branch: KeyType, index: u64) {
        let identity = KeyIdentity {
            account: self.account,
            branch,
            index,
        };
        if !self.sender_key_identities.contains(&identity) {
            self.sender_key_identities.push(identity);
        }
    }

    fn add_script_identity(&mut self, branch: KeyType, index: u64) {
        let identity = KeyIdentity {
            account: self.account,
            branch,
            index,
        };
        if !self.script_key_identities.contains(&identity) {
            self.script_key_identities.push(identity);
        }
    }

    fn sides_are_disjoint(&self) -> bool {
        !self
            .sender_key_identities
            .iter()
            .any(|identity| self.script_key_identities.contains(identity))
    }

    fn unique_identity_count(&self) -> usize {
        self.sender_key_identities.len() +
            self
                .script_key_identities
                .iter()
                .filter(|identity| !self.sender_key_identities.contains(identity))
                .count()
    }

    fn validate_key_identities(&self) -> Result<(), AppSW> {
        if !self.sides_are_disjoint() || self.unique_identity_count() < MIN_UNIQUE_KEYS {
            return Err(AppSW::ScriptOffsetNotUnique);
        }
        Ok(())
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

fn derive_key_from_alpha(
    account: u64,
    data: &[u8],
) -> Result<RistrettoSecretKey, AppSW> {
    if data.len() != 32 {
        return Err(AppSW::WrongApduLength);
    }
    let alpha = derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?;
    let blinding_factor: RistrettoSecretKey = get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();

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
        let partial_script_offset: RistrettoSecretKey =
            get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[0..32])?.into();
        offset_ctx.script_private_key_sum = partial_script_offset;

        return Ok(());
    }

    let payload_offset = 2;
    let end_offset_indexes = payload_offset + offset_ctx.total_offset_indexes;

    // 3. Indexed Sender offset
    if (payload_offset..end_offset_indexes).contains(&(chunk_number as u64)) {
        let (branch, index) = extract_branch_and_index(data)?;
        if !branch.is_script_offset_sender_branch() {
            return Err(AppSW::BadBranchKey);
        }
        let offset = derive_from_bip32_key(offset_ctx.account, index, branch)?;

        offset_ctx.add_sender_identity(branch, index);
        offset_ctx.sender_offset_sum = &offset_ctx.sender_offset_sum + offset;
    }

    // 4. Indexed Script key
    let end_script_indexes = end_offset_indexes + offset_ctx.total_script_indexes;
    if (end_offset_indexes..end_script_indexes).contains(&(chunk_number as u64)) {
        let (branch, index) = extract_branch_and_index(data)?;
        if !branch.is_script_offset_script_branch() {
            return Err(AppSW::BadBranchKey);
        }
        let script_key = derive_from_bip32_key(offset_ctx.account, index, branch)?;

        offset_ctx.add_script_identity(branch, index);
        offset_ctx.script_private_key_sum = &offset_ctx.script_private_key_sum + script_key;
    }

    // 5. Derived sender offsets key
    let end_derived_offset_keys = end_script_indexes + offset_ctx.total_derived_offset_keys;
    if (end_script_indexes..end_derived_offset_keys).contains(&(chunk_number as u64)) {
        let k = derive_key_from_alpha(offset_ctx.account, data)?;

        offset_ctx.add_sender_identity(KeyType::Spend, STATIC_SPEND_INDEX);
        offset_ctx.sender_offset_sum = &offset_ctx.sender_offset_sum + k;
    }

    // 6. Derived script key
    let end_derived_script_keys = end_derived_offset_keys + offset_ctx.total_derived_script_keys;
    if (end_derived_offset_keys..end_derived_script_keys).contains(&(chunk_number as u64)) {
        let k = derive_key_from_alpha(offset_ctx.account, data)?;

        offset_ctx.add_script_identity(KeyType::Spend, STATIC_SPEND_INDEX);
        offset_ctx.script_private_key_sum = &offset_ctx.script_private_key_sum + k
    }

    if more {
        return Ok(());
    }

    // Guard against attacks to extract the spending private key: the two sides must be disjoint in
    // key identity, not merely contain two distinct keys overall.
    offset_ctx.validate_key_identities()?;

    let script_offset = &offset_ctx.script_private_key_sum - &offset_ctx.sender_offset_sum;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_offset.to_vec());
    offset_ctx.reset();

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn context() -> ScriptOffsetCtx {
        let mut ctx = ScriptOffsetCtx::new();
        ctx.account = 7;
        ctx
    }

    #[test]
    fn rejects_one_unique_key() {
        let mut ctx = context();
        ctx.add_script_identity(KeyType::Spend, STATIC_SPEND_INDEX);

        assert_eq!(ctx.validate_key_identities(), Err(AppSW::ScriptOffsetNotUnique));
    }

    #[test]
    fn accepts_two_independent_semantically_valid_keys() {
        let mut ctx = context();
        ctx.add_sender_identity(KeyType::OneSidedSenderOffset, 1);
        ctx.add_script_identity(KeyType::Spend, STATIC_SPEND_INDEX);

        assert_eq!(ctx.validate_key_identities(), Ok(()));
    }

    #[test]
    fn rejects_sender_ab_script_b_overlap() {
        let mut ctx = context();
        ctx.add_sender_identity(KeyType::OneSidedSenderOffset, 1);
        ctx.add_sender_identity(KeyType::Random, 2);
        ctx.add_script_identity(KeyType::Random, 2);

        assert_eq!(ctx.validate_key_identities(), Err(AppSW::ScriptOffsetNotUnique));
    }

    #[test]
    fn rejects_sender_a_script_ab_overlap() {
        let mut ctx = context();
        ctx.add_sender_identity(KeyType::Random, 2);
        ctx.add_script_identity(KeyType::Spend, STATIC_SPEND_INDEX);
        ctx.add_script_identity(KeyType::Random, 2);

        assert_eq!(ctx.validate_key_identities(), Err(AppSW::ScriptOffsetNotUnique));
    }

    #[test]
    fn rejects_static_spend_identity_on_both_sides() {
        let mut ctx = context();
        ctx.add_sender_identity(KeyType::Spend, STATIC_SPEND_INDEX);
        ctx.add_script_identity(KeyType::Spend, STATIC_SPEND_INDEX);

        assert_eq!(ctx.validate_key_identities(), Err(AppSW::ScriptOffsetNotUnique));
    }
}
