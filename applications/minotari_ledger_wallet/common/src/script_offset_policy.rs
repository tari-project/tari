// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Policy governing which Ledger-derived keys a `GetScriptOffset` request may reference.
//!
//! `GetScriptOffset` returns `partial_script_offset + Σ script_keys - Σ sender_offset_keys`. Every term on the right
//! is a secret the host cannot compute for itself, so the response is only safe for as long as the host cannot steer
//! that sum onto a single unknown. Three rules do the steering-prevention:
//!
//! 1. A branch may only appear in the role it is actually used for. In particular the `Spend` branch - the wallet's
//!    master spend key - is never a legitimate operand of a script offset, so it is rejected outright. The spend key
//!    only ever enters the sum blinded, via the derived-key path.
//! 2. No term may be cancelled out by another. A `(branch, index)` identity may appear at most once per request, and so
//!    may a derived key's blinding factor: a term present on both sides drops out of the subtraction while still
//!    looking like it contributed, which is precisely how the offset gets collapsed onto one unknown.
//! 3. A request is only answered once it is complete: chunks arrive in order, exactly once each, and the terminator
//!    lands on the chunk the declared sizes call for. Otherwise a host can declare a large request and terminate early
//!    on a partial sum it fully controls.
//!
//! What those rules protect is [`ScriptOffsetRequestGuard::unique_key_count`], which counts how many secrets the
//! response is actually a function of. Pulling the host-known parts out of the response:
//!
//! ```text
//! response - partial_script_offset - Σ H(blinding factors) = Σ ±k_identity + (n_derived_script - n_derived_sender)·α
//! ```
//!
//! A secret is only hidden if at least two of them survive with a non-zero coefficient, so the count must include the
//! spend key `α` exactly when the two derived-key sections are of *unequal* length - not merely when derived keys are
//! present. Counting presence rather than coefficient would let a host cancel `α` against itself and read the one
//! remaining key straight out of the response.
//!
//! Rules 1 and 2 are expressed over key *identities* rather than over derived scalars: what matters is what the host
//! asked for, not what the device happened to derive.
//!
//! # Residual risk
//!
//! These rules bound which linear combinations the host may request; they do not stop it from asking for many. The
//! device answers each request statelessly and without user interaction, so two requests differing by one term still
//! reveal that term by subtraction. Closing that off needs the offset to be bound to a transaction the device has
//! seen and the user has approved, which is a protocol change rather than a validation change.

use alloc::{fmt, vec::Vec};

use crate::common_types::{AppSW, LedgerKeyBranch};

/// Distinct Ledger keys a script offset must draw on before it will be returned.
pub const MIN_UNIQUE_KEYS: usize = 2;

/// The side of the script offset subtraction that a key contributes to.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ScriptOffsetRole {
    /// Subtracted from the script offset.
    SenderOffset,
    /// Added to the script offset.
    ScriptKey,
}

impl ScriptOffsetRole {
    pub fn as_str(&self) -> &str {
        match self {
            ScriptOffsetRole::SenderOffset => "sender offset",
            ScriptOffsetRole::ScriptKey => "script key",
        }
    }
}

impl fmt::Display for ScriptOffsetRole {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// What the chunk at a given position in a `GetScriptOffset` request carries.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ChunkKind {
    /// The host-supplied starting value of the offset.
    PartialOffset,
    /// A `(branch, index)` key to subtract.
    SenderOffsetIndex,
    /// A `(branch, index)` key to add.
    ScriptKeyIndex,
    /// A blinding factor for a spend-key-derived key to subtract.
    DerivedSenderOffset,
    /// A blinding factor for a spend-key-derived key to add.
    DerivedScriptKey,
}

/// Number of chunks a request declared for each of its four key sections.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct ScriptOffsetTotals {
    pub sender_offset_indexes: u64,
    pub script_key_indexes: u64,
    pub derived_sender_offsets: u64,
    pub derived_script_keys: u64,
}

/// A `GetScriptOffset` request that violates the key policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptOffsetPolicyError {
    /// The branch is not one the given role is allowed to reference.
    BranchNotAllowedForRole {
        branch: LedgerKeyBranch,
        role: ScriptOffsetRole,
    },
    /// The same key identity was referenced more than once in the request.
    DuplicateKeyIdentity { branch: LedgerKeyBranch, index: u64 },
    /// The same derived-key blinding factor was supplied more than once in the request.
    DuplicateBlindingFactor,
    /// A derived-key blinding factor was not a 32 byte scalar.
    MalformedBlindingFactor,
    /// The branch byte does not name a known branch.
    UnknownBranch { branch: u8 },
    /// The declared sizes do not describe a request that can be sent.
    RequestTooLarge,
    /// A chunk arrived out of order, twice, or before the request header.
    UnexpectedChunk { expected: u64, got: u64 },
    /// The request was terminated before every declared chunk had arrived.
    IncompleteRequest { expected_last: u64, got: u64 },
    /// The offset would be a function of fewer than [`MIN_UNIQUE_KEYS`] Ledger keys.
    NotEnoughUniqueKeys { unique: usize },
}

impl ScriptOffsetPolicyError {
    /// The status word the device reports for this violation.
    pub fn app_sw(&self) -> AppSW {
        match self {
            ScriptOffsetPolicyError::BranchNotAllowedForRole { .. } | ScriptOffsetPolicyError::UnknownBranch { .. } => {
                AppSW::BadBranchKey
            },
            ScriptOffsetPolicyError::DuplicateKeyIdentity { .. } |
            ScriptOffsetPolicyError::DuplicateBlindingFactor |
            ScriptOffsetPolicyError::NotEnoughUniqueKeys { .. } => AppSW::ScriptOffsetNotUnique,
            ScriptOffsetPolicyError::MalformedBlindingFactor => AppSW::WrongApduLength,
            ScriptOffsetPolicyError::RequestTooLarge |
            ScriptOffsetPolicyError::UnexpectedChunk { .. } |
            ScriptOffsetPolicyError::IncompleteRequest { .. } => AppSW::WrongApduLength,
        }
    }
}

impl fmt::Display for ScriptOffsetPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScriptOffsetPolicyError::BranchNotAllowedForRole { branch, role } => {
                write!(f, "Branch '{branch}' may not be used as a {role} key")
            },
            ScriptOffsetPolicyError::DuplicateKeyIdentity { branch, index } => {
                write!(f, "Key identity '{branch}/{index}' was referenced more than once")
            },
            ScriptOffsetPolicyError::DuplicateBlindingFactor => {
                write!(f, "A derived key blinding factor was supplied more than once")
            },
            ScriptOffsetPolicyError::MalformedBlindingFactor => {
                write!(f, "A derived key blinding factor was not a 32 byte scalar")
            },
            ScriptOffsetPolicyError::UnknownBranch { branch } => write!(f, "Unknown key branch '{branch}'"),
            ScriptOffsetPolicyError::RequestTooLarge => write!(f, "Script offset request declares too many chunks"),
            ScriptOffsetPolicyError::UnexpectedChunk { expected, got } => {
                write!(f, "Expected chunk {expected}, got {got}")
            },
            ScriptOffsetPolicyError::IncompleteRequest { expected_last, got } => {
                write!(
                    f,
                    "Request terminated on chunk {got}, expected it to end on {expected_last}"
                )
            },
            ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique } => {
                write!(
                    f,
                    "Script offset draws on {unique} unique key(s), need {MIN_UNIQUE_KEYS}"
                )
            },
        }
    }
}

/// Whether `branch` may be referenced by an indexed key in the given role.
///
/// The allow-list mirrors how the wallet actually builds a script offset: sender offset keys come from the
/// `OneSidedSenderOffset` branch for ordinary transactions and the `Random` branch for pre-mine spends, and the only
/// indexed script key is a `PreMine` one. Ordinary script keys are not indexed at all - they travel as derived keys.
///
/// `Spend` and `MetadataEphemeralNonce` are excluded from both roles because the wallet never uses them here - the
/// former is the master spend key and has no business being an addressable operand at all.
///
/// Note that this is an allow-list of *observed usage*, not a claim that the permitted branches are harmless in the
/// abstract. `Random` is used both for pre-mine sender offset keys and for signing nonces
/// (`minotari_console_wallet/src/automation/commands.rs`), so a `Random` key recovered from this handler may also be
/// a nonce. That branch overloading is worth removing on its own account; until it is, it raises the cost of the
/// residual multi-request risk described at the top of this module.
pub fn branch_is_valid_for_role(branch: LedgerKeyBranch, role: ScriptOffsetRole) -> bool {
    match role {
        ScriptOffsetRole::SenderOffset => {
            matches!(branch, LedgerKeyBranch::OneSidedSenderOffset | LedgerKeyBranch::Random)
        },
        ScriptOffsetRole::ScriptKey => matches!(branch, LedgerKeyBranch::PreMine),
    }
}

/// Validate a complete `GetScriptOffset` request before it is sent.
///
/// This replays the request through the very same [`ScriptOffsetRequestGuard`] the device drives chunk by chunk, so
/// the host cannot pass a check the device would fail: every rule, including the declared-size limit and the minimum
/// key count, is applied here too.
pub fn validate_script_offset_request(
    sender_offset_indexes: &[(LedgerKeyBranch, u64)],
    script_key_indexes: &[(LedgerKeyBranch, u64)],
    derived_sender_offsets: &[&[u8]],
    derived_script_keys: &[&[u8]],
    max_chunk: u64,
) -> Result<(), ScriptOffsetPolicyError> {
    let totals = ScriptOffsetTotals {
        sender_offset_indexes: sender_offset_indexes.len() as u64,
        script_key_indexes: script_key_indexes.len() as u64,
        derived_sender_offsets: derived_sender_offsets.len() as u64,
        derived_script_keys: derived_script_keys.len() as u64,
    };

    let mut guard = ScriptOffsetRequestGuard::new();
    guard.begin(totals, max_chunk)?;

    // Chunk 1 is the partial offset; the key sections follow in the order the device reads them.
    let mut chunk = 1u64;
    guard.classify_chunk(chunk)?;
    for (branch, index) in sender_offset_indexes {
        chunk += 1;
        guard.classify_chunk(chunk)?;
        guard.record_indexed_key(branch.as_byte(), *index, ScriptOffsetRole::SenderOffset)?;
    }
    for (branch, index) in script_key_indexes {
        chunk += 1;
        guard.classify_chunk(chunk)?;
        guard.record_indexed_key(branch.as_byte(), *index, ScriptOffsetRole::ScriptKey)?;
    }
    for blinding_factor in derived_sender_offsets {
        chunk += 1;
        guard.classify_chunk(chunk)?;
        guard.record_derived_key(blinding_factor, ScriptOffsetRole::SenderOffset)?;
    }
    for blinding_factor in derived_script_keys {
        chunk += 1;
        guard.classify_chunk(chunk)?;
        guard.record_derived_key(blinding_factor, ScriptOffsetRole::ScriptKey)?;
    }

    guard.finish(chunk)
}

/// Incremental enforcement of the script offset key policy, as the device sees a request: one chunk at a time, with
/// no memory of what came before beyond what is held here.
///
/// The device pairs this with the actual scalar arithmetic; keeping the two apart is deliberate, so the rules that
/// decide whether an offset may be returned can be exercised without a Ledger toolchain or a physical device.
#[derive(Debug, Clone, Default)]
pub struct ScriptOffsetRequestGuard {
    totals: ScriptOffsetTotals,
    /// Identities - `(branch, index)` - of every indexed key the host has asked for, on either side of the
    /// subtraction. Tracking identities rather than derived scalars is what makes a key referenced from both sides
    /// detectable: such a key cancels out of the offset while still looking like extra entropy.
    key_identities: Vec<(u8, u64)>,
    /// Blinding factors of the derived keys the host has supplied, on either side. Each derived key contributes
    /// `H(blinding factor) + α`, and `H(..)` is host-computable, so the same factor on both sides cancels the spend
    /// key against itself just as surely as a repeated identity cancels an indexed key.
    derived_blinding_factors: Vec<[u8; 32]>,
    /// How many derived keys landed on each side. The spend key's coefficient in the response is the difference
    /// between the two, so these have to be counted separately rather than collapsed into a "spend key was used"
    /// flag - see the module docs.
    derived_script_keys: usize,
    derived_sender_offsets: usize,
    /// Next chunk number expected.
    next_chunk: u64,
    /// Chunk number that must carry the terminator for this request.
    last_chunk: u64,
    /// Whether a header chunk has been seen, so stray chunks cannot be accumulated into a stale request.
    started: bool,
}

impl ScriptOffsetRequestGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Discard all state. Called on the header chunk and whenever a request is abandoned, so a rejected or completed
    /// request can never contribute to the next one.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Begin a new request from its declared section sizes.
    ///
    /// `max_chunk` is the largest chunk number the transport can address, so the declared sizes cannot describe a
    /// request that could never be delivered in full.
    pub fn begin(&mut self, totals: ScriptOffsetTotals, max_chunk: u64) -> Result<(), ScriptOffsetPolicyError> {
        self.reset();

        // The declared sizes decide how every later chunk is interpreted, and they arrive unvalidated from the host,
        // so resolve them into a chunk count with overflow checks before anything relies on them.
        let last_chunk = [
            totals.sender_offset_indexes,
            totals.script_key_indexes,
            totals.derived_sender_offsets,
            totals.derived_script_keys,
        ]
        .iter()
        .try_fold(1u64, |acc, n| acc.checked_add(*n))
        .ok_or(ScriptOffsetPolicyError::RequestTooLarge)?;
        if last_chunk > max_chunk {
            return Err(ScriptOffsetPolicyError::RequestTooLarge);
        }

        self.totals = totals;
        self.last_chunk = last_chunk;
        self.next_chunk = 1;
        self.started = true;
        Ok(())
    }

    /// Accept the next chunk and say what it carries.
    ///
    /// Chunks must arrive in order and exactly once, so a partially accumulated request can never be evaluated.
    pub fn classify_chunk(&mut self, chunk_number: u64) -> Result<ChunkKind, ScriptOffsetPolicyError> {
        if !self.started || chunk_number != self.next_chunk || chunk_number > self.last_chunk {
            return Err(ScriptOffsetPolicyError::UnexpectedChunk {
                expected: self.next_chunk,
                got: chunk_number,
            });
        }
        self.next_chunk += 1;

        if chunk_number == 1 {
            return Ok(ChunkKind::PartialOffset);
        }

        // Section boundaries. These cannot overflow: `begin` already summed the same totals successfully.
        let end_offset_indexes = 2 + self.totals.sender_offset_indexes;
        let end_script_indexes = end_offset_indexes + self.totals.script_key_indexes;
        let end_derived_offsets = end_script_indexes + self.totals.derived_sender_offsets;

        if chunk_number < end_offset_indexes {
            Ok(ChunkKind::SenderOffsetIndex)
        } else if chunk_number < end_script_indexes {
            Ok(ChunkKind::ScriptKeyIndex)
        } else if chunk_number < end_derived_offsets {
            Ok(ChunkKind::DerivedSenderOffset)
        } else {
            Ok(ChunkKind::DerivedScriptKey)
        }
    }

    /// Record an indexed key the host has asked for, enforcing the role allow-list and identity uniqueness.
    ///
    /// Returns the branch so the caller can derive from it, having established that it is allowed here.
    pub fn record_indexed_key(
        &mut self,
        branch: u8,
        index: u64,
        role: ScriptOffsetRole,
    ) -> Result<LedgerKeyBranch, ScriptOffsetPolicyError> {
        let branch = LedgerKeyBranch::from_byte(branch).ok_or(ScriptOffsetPolicyError::UnknownBranch { branch })?;
        if !branch_is_valid_for_role(branch, role) {
            return Err(ScriptOffsetPolicyError::BranchNotAllowedForRole { branch, role });
        }

        // One rule for both sides: the same key twice on one side scales it, and the same key on both sides cancels
        // it. Neither has a legitimate use, and both hand the host the coefficients of the returned combination.
        let identity = (branch.as_byte(), index);
        if self.key_identities.contains(&identity) {
            return Err(ScriptOffsetPolicyError::DuplicateKeyIdentity { branch, index });
        }
        self.key_identities.push(identity);
        Ok(branch)
    }

    /// Record a derived key the host has asked for, enforcing that its blinding factor is not reused.
    ///
    /// Every derived key is `H(blinding_factor) + α` for the one spend key `α`, so these are not tracked as distinct
    /// secrets; what matters is which side each landed on.
    pub fn record_derived_key(
        &mut self,
        blinding_factor: &[u8],
        role: ScriptOffsetRole,
    ) -> Result<(), ScriptOffsetPolicyError> {
        let blinding_factor: [u8; 32] = blinding_factor
            .try_into()
            .map_err(|_| ScriptOffsetPolicyError::MalformedBlindingFactor)?;
        if self.derived_blinding_factors.contains(&blinding_factor) {
            return Err(ScriptOffsetPolicyError::DuplicateBlindingFactor);
        }
        self.derived_blinding_factors.push(blinding_factor);

        match role {
            ScriptOffsetRole::SenderOffset => self.derived_sender_offsets += 1,
            ScriptOffsetRole::ScriptKey => self.derived_script_keys += 1,
        }
        Ok(())
    }

    /// How many secrets the response would actually be a function of.
    ///
    /// Each recorded identity appears exactly once, so each contributes a coefficient of ±1. The spend key's
    /// coefficient is `derived_script_keys - derived_sender_offsets`, so it only counts when the two sides carry
    /// different numbers of derived keys; balanced derived keys cancel it out entirely and it hides nothing.
    pub fn unique_key_count(&self) -> usize {
        let spend_key_survives = self.derived_script_keys != self.derived_sender_offsets;
        self.key_identities
            .len()
            .saturating_add(usize::from(spend_key_survives))
    }

    /// Check that the request terminating on `chunk_number` may be answered.
    pub fn finish(&self, chunk_number: u64) -> Result<(), ScriptOffsetPolicyError> {
        if !self.started || chunk_number != self.last_chunk {
            return Err(ScriptOffsetPolicyError::IncompleteRequest {
                expected_last: self.last_chunk,
                got: chunk_number,
            });
        }
        let unique = self.unique_key_count();
        if unique < MIN_UNIQUE_KEYS {
            return Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique });
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use alloc::{vec, vec::Vec};

    use super::{
        ChunkKind,
        ScriptOffsetPolicyError,
        ScriptOffsetRequestGuard,
        ScriptOffsetRole,
        ScriptOffsetTotals,
        branch_is_valid_for_role,
        validate_script_offset_request,
    };
    use crate::common_types::{AppSW, LedgerKeyBranch};

    /// Mirrors `MAX_PAYLOADS` in the device application.
    const MAX_CHUNK: u64 = 250;
    /// Mirrors `STATIC_SPEND_INDEX` in the device application: where the master spend key lives.
    const STATIC_SPEND_INDEX: u64 = 42;

    const ALL_BRANCHES: [LedgerKeyBranch; 5] = [
        LedgerKeyBranch::MetadataEphemeralNonce,
        LedgerKeyBranch::OneSidedSenderOffset,
        LedgerKeyBranch::Random,
        LedgerKeyBranch::PreMine,
        LedgerKeyBranch::Spend,
    ];

    /// A distinct 32 byte blinding factor.
    fn bf(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    /// `n` distinct blinding factors, starting at `first`.
    fn bfs(first: u8, n: u8) -> Vec<[u8; 32]> {
        (0..n).map(|i| bf(first + i)).collect()
    }

    /// Replay a complete request through the guard exactly as the device handler drives it, and report whether the
    /// device would have returned an offset.
    fn run_request(
        sender: &[(LedgerKeyBranch, u64)],
        script: &[(LedgerKeyBranch, u64)],
        derived_sender: &[[u8; 32]],
        derived_script: &[[u8; 32]],
    ) -> Result<(), ScriptOffsetPolicyError> {
        let mut guard = ScriptOffsetRequestGuard::new();
        guard.begin(
            ScriptOffsetTotals {
                sender_offset_indexes: sender.len() as u64,
                script_key_indexes: script.len() as u64,
                derived_sender_offsets: derived_sender.len() as u64,
                derived_script_keys: derived_script.len() as u64,
            },
            MAX_CHUNK,
        )?;

        let last =
            1 + sender.len() as u64 + script.len() as u64 + derived_sender.len() as u64 + derived_script.len() as u64;
        let mut sender_keys = sender.iter();
        let mut script_keys = script.iter();
        let mut sender_factors = derived_sender.iter();
        let mut script_factors = derived_script.iter();
        for chunk in 1..=last {
            match guard.classify_chunk(chunk)? {
                ChunkKind::PartialOffset => {},
                ChunkKind::SenderOffsetIndex => {
                    let (branch, index) = sender_keys.next().expect("sender key for chunk");
                    guard.record_indexed_key(branch.as_byte(), *index, ScriptOffsetRole::SenderOffset)?;
                },
                ChunkKind::ScriptKeyIndex => {
                    let (branch, index) = script_keys.next().expect("script key for chunk");
                    guard.record_indexed_key(branch.as_byte(), *index, ScriptOffsetRole::ScriptKey)?;
                },
                ChunkKind::DerivedSenderOffset => {
                    let factor = sender_factors.next().expect("blinding factor for chunk");
                    guard.record_derived_key(factor, ScriptOffsetRole::SenderOffset)?;
                },
                ChunkKind::DerivedScriptKey => {
                    let factor = script_factors.next().expect("blinding factor for chunk");
                    guard.record_derived_key(factor, ScriptOffsetRole::ScriptKey)?;
                },
            }
        }
        guard.finish(last)
    }

    // ---------------------------------------------------------------------------------------------------------
    // Requests the wallet legitimately makes. These must keep working; an over-tight guard is a wallet that cannot
    // spend.
    // ---------------------------------------------------------------------------------------------------------

    #[test]
    fn ordinary_transaction_is_accepted() {
        // One sender offset key from the one-sided branch, script keys travelling as derived keys.
        assert_eq!(
            run_request(&[(LedgerKeyBranch::OneSidedSenderOffset, 7)], &[], &[], &bfs(0x40, 1)),
            Ok(())
        );
    }

    #[test]
    fn multi_input_transaction_is_accepted() {
        // A transaction with several inputs sends one derived script key per input. They all fold in the same spend
        // key, which must not be mistaken for the request leaning on a single key over and over.
        for inputs in 1..8 {
            assert_eq!(
                run_request(
                    &[(LedgerKeyBranch::OneSidedSenderOffset, 1)],
                    &[],
                    &[],
                    &bfs(0x40, inputs)
                ),
                Ok(()),
                "a {inputs}-input transaction must be signable"
            );
        }
    }

    #[test]
    fn multiple_distinct_sender_offset_keys_are_accepted() {
        let sender = [
            (LedgerKeyBranch::OneSidedSenderOffset, 1),
            (LedgerKeyBranch::OneSidedSenderOffset, 2),
            (LedgerKeyBranch::Random, 1),
        ];
        assert_eq!(run_request(&sender, &[], &[], &bfs(0x40, 2)), Ok(()));
    }

    #[test]
    fn pre_mine_spend_is_accepted() {
        assert_eq!(
            run_request(
                &[(LedgerKeyBranch::Random, 99)],
                &[(LedgerKeyBranch::PreMine, 3)],
                &[],
                &[]
            ),
            Ok(())
        );
    }

    // ---------------------------------------------------------------------------------------------------------
    // GHSA-g9mx-9jmr-65vq: extracting the master spend key.
    // ---------------------------------------------------------------------------------------------------------

    #[test]
    fn advisory_cross_side_cancellation_is_rejected() {
        // sender = [Spend/42, Spend/43], script = [Spend/43] returns `partial - Spend/42`.
        let sender = [
            (LedgerKeyBranch::Spend, STATIC_SPEND_INDEX),
            (LedgerKeyBranch::Spend, 43),
        ];
        let script = [(LedgerKeyBranch::Spend, 43)];
        assert_eq!(
            run_request(&sender, &script, &[], &[]),
            Err(ScriptOffsetPolicyError::BranchNotAllowedForRole {
                branch: LedgerKeyBranch::Spend,
                role: ScriptOffsetRole::SenderOffset,
            })
        );
    }

    #[test]
    fn advisory_negative_control_still_rejected() {
        // The advisory's baseline: the target key on its own. Rejected before the fix too, and still rejected.
        assert!(run_request(&[(LedgerKeyBranch::Spend, STATIC_SPEND_INDEX)], &[], &[], &[]).is_err());
    }

    #[test]
    fn spend_branch_is_never_a_valid_operand() {
        for role in [ScriptOffsetRole::SenderOffset, ScriptOffsetRole::ScriptKey] {
            assert!(
                !branch_is_valid_for_role(LedgerKeyBranch::Spend, role),
                "Spend must not be usable as a {role} key"
            );
        }

        // Reachable from neither side, at any index, however much legitimate-looking company it is given.
        let target = (LedgerKeyBranch::Spend, STATIC_SPEND_INDEX);
        assert!(run_request(&[target], &[], &[], &[]).is_err());
        assert!(run_request(&[], &[target], &[], &[]).is_err());
        assert!(
            run_request(
                &[target, (LedgerKeyBranch::Random, 0)],
                &[(LedgerKeyBranch::PreMine, 1)],
                &[],
                &[]
            )
            .is_err()
        );
        assert!(run_request(&[(LedgerKeyBranch::Random, 0), target], &[], &[], &bfs(0x40, 2)).is_err());
        assert!(run_request(&[(LedgerKeyBranch::Spend, 0)], &[], &[], &bfs(0x40, 1)).is_err());
    }

    #[test]
    fn ephemeral_nonce_branch_is_never_a_valid_operand() {
        for role in [ScriptOffsetRole::SenderOffset, ScriptOffsetRole::ScriptKey] {
            assert!(!branch_is_valid_for_role(LedgerKeyBranch::MetadataEphemeralNonce, role));
        }
        assert!(run_request(&[(LedgerKeyBranch::MetadataEphemeralNonce, 1)], &[], &[], &bfs(0x40, 1)).is_err());
    }

    #[test]
    fn role_allow_list_is_exactly_what_the_wallet_uses() {
        let allowed = |role| {
            ALL_BRANCHES
                .iter()
                .copied()
                .filter(|b| branch_is_valid_for_role(*b, role))
                .collect::<Vec<_>>()
        };
        assert_eq!(allowed(ScriptOffsetRole::SenderOffset), vec![
            LedgerKeyBranch::OneSidedSenderOffset,
            LedgerKeyBranch::Random,
        ]);
        assert_eq!(allowed(ScriptOffsetRole::ScriptKey), vec![LedgerKeyBranch::PreMine]);
    }

    #[test]
    fn unknown_branch_byte_is_rejected() {
        let mut guard = ScriptOffsetRequestGuard::new();
        assert_eq!(
            guard.record_indexed_key(0xFE, 1, ScriptOffsetRole::SenderOffset),
            Err(ScriptOffsetPolicyError::UnknownBranch { branch: 0xFE })
        );
    }

    // ---------------------------------------------------------------------------------------------------------
    // Collapsing the offset onto a single key by other means.
    // ---------------------------------------------------------------------------------------------------------

    #[test]
    fn a_single_key_request_is_rejected() {
        assert_eq!(
            run_request(&[(LedgerKeyBranch::Random, 42)], &[], &[], &[]),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 })
        );
        assert_eq!(
            run_request(&[], &[(LedgerKeyBranch::PreMine, 1)], &[], &[]),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 })
        );
    }

    #[test]
    fn derived_keys_alone_are_one_key_however_many_there_are() {
        // Every derived key folds in the same spend key, so a request made only of derived keys leans on exactly one
        // Ledger secret no matter how long it is.
        for derived in 1..6 {
            assert_eq!(
                run_request(&[], &[], &[], &bfs(0x40, derived)),
                Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 }),
                "{derived} derived script keys still only draw on the spend key"
            );
        }
        assert_eq!(
            run_request(&[], &[], &bfs(0x10, 2), &bfs(0x40, 3)),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 })
        );
    }

    // The spend key must count only when it actually survives the subtraction. Every case here would otherwise pass
    // MIN_UNIQUE_KEYS on a presence flag while the response reduced to one indexed key in the clear.
    #[test]
    fn balanced_derived_keys_cancel_the_spend_key_and_hide_nothing() {
        // Same blinding factor on both sides: the two derived terms are literally identical and cancel.
        assert_eq!(
            run_request(&[], &[(LedgerKeyBranch::PreMine, 3)], &[bf(0x10)], &[bf(0x10)]),
            Err(ScriptOffsetPolicyError::DuplicateBlindingFactor)
        );

        // Different blinding factors, but still one derived key a side: `H(b1) - H(b2)` is host-known and the spend
        // key still cancels, so the response is `partial + known - k`. Deduping factors alone would miss this.
        assert_eq!(
            run_request(&[(LedgerKeyBranch::Random, 9)], &[], &[bf(0x10)], &[bf(0x40)]),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 })
        );

        // Any equal split cancels, however many keys are involved.
        for n in 1..5 {
            assert_eq!(
                run_request(&[(LedgerKeyBranch::Random, 9)], &[], &bfs(0x10, n), &bfs(0x40, n)),
                Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 }),
                "{n} derived keys a side must cancel the spend key"
            );
        }
    }

    #[test]
    fn unbalanced_derived_keys_keep_the_spend_key_in_play() {
        // One more derived key on one side than the other leaves the spend key with a non-zero coefficient, so the
        // response genuinely hides two secrets and may be returned.
        assert_eq!(
            run_request(&[(LedgerKeyBranch::Random, 9)], &[], &bfs(0x10, 2), &bfs(0x40, 3)),
            Ok(())
        );
        assert_eq!(
            run_request(&[(LedgerKeyBranch::Random, 9)], &[], &bfs(0x10, 3), &bfs(0x40, 2)),
            Ok(())
        );
    }

    #[test]
    fn a_blinding_factor_may_not_be_reused_on_one_side() {
        assert_eq!(
            run_request(&[(LedgerKeyBranch::Random, 9)], &[], &[], &[bf(0x40), bf(0x40)]),
            Err(ScriptOffsetPolicyError::DuplicateBlindingFactor)
        );
    }

    #[test]
    fn a_malformed_blinding_factor_is_rejected() {
        let mut guard = ScriptOffsetRequestGuard::new();
        assert_eq!(
            guard.record_derived_key(&[0u8; 31], ScriptOffsetRole::ScriptKey),
            Err(ScriptOffsetPolicyError::MalformedBlindingFactor)
        );
    }

    #[test]
    fn an_empty_request_is_rejected() {
        assert_eq!(
            run_request(&[], &[], &[], &[]),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 0 })
        );
    }

    #[test]
    fn duplicate_identity_is_rejected() {
        // Repeating a key scales its coefficient, which is another way to steer the returned combination.
        assert_eq!(
            run_request(
                &[(LedgerKeyBranch::Random, 4), (LedgerKeyBranch::Random, 4)],
                &[(LedgerKeyBranch::PreMine, 1)],
                &[],
                &[]
            ),
            Err(ScriptOffsetPolicyError::DuplicateKeyIdentity {
                branch: LedgerKeyBranch::Random,
                index: 4,
            })
        );
    }

    #[test]
    fn cross_side_duplicates_are_unreachable_and_rejected() {
        // The role allow-list gives the two sides disjoint branches, so a key cannot be placed on both sides at all;
        // whichever side is wrong fails the role rule first. The identity check behind it is defence in depth for if
        // that allow-list ever widens.
        assert!(matches!(
            run_request(
                &[(LedgerKeyBranch::PreMine, 5)],
                &[(LedgerKeyBranch::PreMine, 5)],
                &[],
                &[]
            ),
            Err(ScriptOffsetPolicyError::BranchNotAllowedForRole { .. })
        ));

        let mut guard = ScriptOffsetRequestGuard::new();
        guard
            .record_indexed_key(LedgerKeyBranch::Random.as_byte(), 5, ScriptOffsetRole::SenderOffset)
            .unwrap();
        assert_eq!(
            guard.record_indexed_key(LedgerKeyBranch::Random.as_byte(), 5, ScriptOffsetRole::SenderOffset),
            Err(ScriptOffsetPolicyError::DuplicateKeyIdentity {
                branch: LedgerKeyBranch::Random,
                index: 5,
            })
        );
    }

    #[test]
    fn same_index_on_different_branches_is_not_a_duplicate() {
        assert_eq!(
            run_request(
                &[(LedgerKeyBranch::OneSidedSenderOffset, 5), (LedgerKeyBranch::Random, 5)],
                &[(LedgerKeyBranch::PreMine, 5)],
                &[],
                &[]
            ),
            Ok(())
        );
    }

    // ---------------------------------------------------------------------------------------------------------
    // Request framing: a partially delivered request must never be answered.
    // ---------------------------------------------------------------------------------------------------------

    #[test]
    fn chunks_must_arrive_in_order() {
        let mut guard = ScriptOffsetRequestGuard::new();
        guard
            .begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: 2,
                    script_key_indexes: 1,
                    ..Default::default()
                },
                MAX_CHUNK,
            )
            .unwrap();
        assert_eq!(guard.classify_chunk(1), Ok(ChunkKind::PartialOffset));
        assert_eq!(
            guard.classify_chunk(3),
            Err(ScriptOffsetPolicyError::UnexpectedChunk { expected: 2, got: 3 })
        );
    }

    #[test]
    fn chunks_cannot_be_replayed() {
        let mut guard = ScriptOffsetRequestGuard::new();
        guard
            .begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: 2,
                    ..Default::default()
                },
                MAX_CHUNK,
            )
            .unwrap();
        assert_eq!(guard.classify_chunk(1), Ok(ChunkKind::PartialOffset));
        assert_eq!(guard.classify_chunk(2), Ok(ChunkKind::SenderOffsetIndex));
        assert_eq!(
            guard.classify_chunk(2),
            Err(ScriptOffsetPolicyError::UnexpectedChunk { expected: 3, got: 2 })
        );
    }

    #[test]
    fn chunks_beyond_the_declared_request_are_rejected() {
        let mut guard = ScriptOffsetRequestGuard::new();
        guard
            .begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: 1,
                    ..Default::default()
                },
                MAX_CHUNK,
            )
            .unwrap();
        assert!(guard.classify_chunk(1).is_ok());
        assert!(guard.classify_chunk(2).is_ok());
        assert!(guard.classify_chunk(3).is_err());
    }

    #[test]
    fn a_chunk_before_the_header_is_rejected() {
        let mut guard = ScriptOffsetRequestGuard::new();
        assert!(guard.classify_chunk(1).is_err());
        assert!(guard.finish(1).is_err());
    }

    #[test]
    fn terminating_early_is_rejected() {
        // Declare a long request, deliver two chunks, then claim to be done: the accumulated sum is not the one the
        // declared sizes describe.
        let mut guard = ScriptOffsetRequestGuard::new();
        guard
            .begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: 1,
                    script_key_indexes: 1,
                    derived_script_keys: 3,
                    ..Default::default()
                },
                MAX_CHUNK,
            )
            .unwrap();
        guard.classify_chunk(1).unwrap();
        guard.classify_chunk(2).unwrap();
        guard
            .record_indexed_key(LedgerKeyBranch::Random.as_byte(), 1, ScriptOffsetRole::SenderOffset)
            .unwrap();
        guard.classify_chunk(3).unwrap();
        guard
            .record_indexed_key(LedgerKeyBranch::PreMine.as_byte(), 1, ScriptOffsetRole::ScriptKey)
            .unwrap();
        // Two unique keys are present, so only the completeness check stands between this and an answer.
        assert_eq!(guard.unique_key_count(), 2);
        assert_eq!(
            guard.finish(3),
            Err(ScriptOffsetPolicyError::IncompleteRequest {
                expected_last: 6,
                got: 3
            })
        );
    }

    #[test]
    fn declared_sizes_that_overflow_are_rejected() {
        let mut guard = ScriptOffsetRequestGuard::new();
        assert_eq!(
            guard.begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: u64::MAX,
                    script_key_indexes: 2,
                    ..Default::default()
                },
                MAX_CHUNK,
            ),
            Err(ScriptOffsetPolicyError::RequestTooLarge)
        );
    }

    #[test]
    fn declared_sizes_beyond_the_transport_limit_are_rejected() {
        let mut guard = ScriptOffsetRequestGuard::new();
        assert_eq!(
            guard.begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: MAX_CHUNK,
                    ..Default::default()
                },
                MAX_CHUNK,
            ),
            Err(ScriptOffsetPolicyError::RequestTooLarge)
        );
        // One less fits exactly.
        assert_eq!(
            guard.begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: MAX_CHUNK - 1,
                    ..Default::default()
                },
                MAX_CHUNK,
            ),
            Ok(())
        );
    }

    #[test]
    fn reset_clears_every_trace_of_the_previous_request() {
        let mut guard = ScriptOffsetRequestGuard::new();
        guard
            .begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: 1,
                    ..Default::default()
                },
                MAX_CHUNK,
            )
            .unwrap();
        guard.classify_chunk(1).unwrap();
        guard.classify_chunk(2).unwrap();
        guard
            .record_indexed_key(LedgerKeyBranch::Random.as_byte(), 1, ScriptOffsetRole::SenderOffset)
            .unwrap();
        guard
            .record_derived_key(&bf(0x40), ScriptOffsetRole::ScriptKey)
            .unwrap();
        assert_eq!(guard.unique_key_count(), 2);

        guard.reset();
        assert_eq!(guard.unique_key_count(), 0);
        assert!(guard.classify_chunk(1).is_err());
        // The same identity is free again in a fresh request.
        guard
            .begin(
                ScriptOffsetTotals {
                    sender_offset_indexes: 1,
                    ..Default::default()
                },
                MAX_CHUNK,
            )
            .unwrap();
        assert!(
            guard
                .record_indexed_key(LedgerKeyBranch::Random.as_byte(), 1, ScriptOffsetRole::SenderOffset)
                .is_ok()
        );
    }

    // ---------------------------------------------------------------------------------------------------------
    // Host-side pre-check and status word mapping.
    // ---------------------------------------------------------------------------------------------------------

    /// A request shape: indexed sender offsets, indexed script keys, derived sender offsets, derived script keys.
    type Case<'a> = (
        &'a [(LedgerKeyBranch, u64)],
        &'a [(LedgerKeyBranch, u64)],
        Vec<[u8; 32]>,
        Vec<[u8; 32]>,
    );

    /// The host pre-check, over the same request shape `run_request` replays.
    fn run_host_check(
        sender: &[(LedgerKeyBranch, u64)],
        script: &[(LedgerKeyBranch, u64)],
        derived_sender: &[[u8; 32]],
        derived_script: &[[u8; 32]],
    ) -> Result<(), ScriptOffsetPolicyError> {
        let ds: Vec<&[u8]> = derived_sender.iter().map(|f| f.as_slice()).collect();
        let dk: Vec<&[u8]> = derived_script.iter().map(|f| f.as_slice()).collect();
        validate_script_offset_request(sender, script, &ds, &dk, MAX_CHUNK)
    }

    /// The host pre-check exists so nothing gets past it only to be refused on-device. Hold the two to the same
    /// verdict over every request shape the rest of this module exercises.
    #[test]
    fn host_side_check_agrees_with_the_device() {
        let one_sided = [(LedgerKeyBranch::OneSidedSenderOffset, 7)];
        let pre_mine_sender = [(LedgerKeyBranch::Random, 99)];
        let pre_mine_script = [(LedgerKeyBranch::PreMine, 3)];
        let advisory_sender = [
            (LedgerKeyBranch::Spend, STATIC_SPEND_INDEX),
            (LedgerKeyBranch::Spend, 43),
        ];
        let advisory_script = [(LedgerKeyBranch::Spend, 43)];
        let repeated = [(LedgerKeyBranch::Random, 4), (LedgerKeyBranch::Random, 4)];

        let cases: [Case<'_>; 8] = [
            // Legitimate shapes.
            (&one_sided, &[], vec![], bfs(0x40, 1)),
            (&one_sided, &[], vec![], bfs(0x40, 5)),
            (&pre_mine_sender, &pre_mine_script, vec![], vec![]),
            // Refused shapes: the advisory, a lone key, derived-only, balanced derived keys, a repeated identity.
            (&advisory_sender, &advisory_script, vec![], vec![]),
            (&one_sided, &[], vec![], vec![]),
            (&[], &[], vec![], bfs(0x40, 3)),
            (&pre_mine_script, &[], bfs(0x10, 1), bfs(0x40, 1)),
            (&repeated, &[], vec![], bfs(0x40, 1)),
        ];

        for (sender, script, derived_sender, derived_script) in cases {
            let device = run_request(sender, script, &derived_sender, &derived_script);
            let host = run_host_check(sender, script, &derived_sender, &derived_script);
            assert_eq!(
                device, host,
                "host and device disagree on sender={sender:?} script={script:?}"
            );
        }
    }

    #[test]
    fn host_side_check_enforces_the_chunk_limit() {
        let sender: Vec<(LedgerKeyBranch, u64)> = (0..MAX_CHUNK).map(|i| (LedgerKeyBranch::Random, i)).collect();
        assert_eq!(
            run_host_check(&sender, &[], &[], &[]),
            Err(ScriptOffsetPolicyError::RequestTooLarge)
        );
    }

    #[test]
    fn violations_map_to_the_expected_status_words() {
        assert_eq!(
            ScriptOffsetPolicyError::BranchNotAllowedForRole {
                branch: LedgerKeyBranch::Spend,
                role: ScriptOffsetRole::SenderOffset,
            }
            .app_sw(),
            AppSW::BadBranchKey
        );
        assert_eq!(
            ScriptOffsetPolicyError::UnknownBranch { branch: 0 }.app_sw(),
            AppSW::BadBranchKey
        );
        assert_eq!(
            ScriptOffsetPolicyError::DuplicateKeyIdentity {
                branch: LedgerKeyBranch::Random,
                index: 1,
            }
            .app_sw(),
            AppSW::ScriptOffsetNotUnique
        );
        assert_eq!(
            ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 }.app_sw(),
            AppSW::ScriptOffsetNotUnique
        );
        assert_eq!(
            ScriptOffsetPolicyError::RequestTooLarge.app_sw(),
            AppSW::WrongApduLength
        );
        assert_eq!(
            ScriptOffsetPolicyError::UnexpectedChunk { expected: 1, got: 2 }.app_sw(),
            AppSW::WrongApduLength
        );
        assert_eq!(
            ScriptOffsetPolicyError::IncompleteRequest {
                expected_last: 3,
                got: 2
            }
            .app_sw(),
            AppSW::WrongApduLength
        );
    }
}
