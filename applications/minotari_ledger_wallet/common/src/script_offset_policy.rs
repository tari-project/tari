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
//! 2. A `(branch, index)` identity may appear at most once per request. A key present on both sides cancels out of the
//!    subtraction while still counting towards the "enough distinct keys" guard, which is precisely how the offset gets
//!    collapsed onto one unknown.
//! 3. A request is only answered once it is complete: chunks arrive in order, exactly once each, and the terminator
//!    lands on the chunk the declared sizes call for. Otherwise a host can declare a large request and terminate early
//!    on a partial sum it fully controls.
//!
//! The first two rules are expressed over key *identities* rather than over derived scalars: what matters is what the
//! host asked for, not what the device happened to derive.
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
            ScriptOffsetPolicyError::NotEnoughUniqueKeys { .. } => AppSW::ScriptOffsetNotUnique,
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
/// `Spend` is excluded from both roles: it is the master spend key and has no business being an addressable operand.
/// `MetadataEphemeralNonce` is excluded because nonces must never be summed into a value returned to the host.
pub fn branch_is_valid_for_role(branch: LedgerKeyBranch, role: ScriptOffsetRole) -> bool {
    match role {
        ScriptOffsetRole::SenderOffset => {
            matches!(branch, LedgerKeyBranch::OneSidedSenderOffset | LedgerKeyBranch::Random)
        },
        ScriptOffsetRole::ScriptKey => matches!(branch, LedgerKeyBranch::PreMine),
    }
}

/// Validate the indexed key identities of a complete `GetScriptOffset` request.
///
/// This is the whole-request form of the first two rules, used by the host to fail early. The device applies the same
/// rules incrementally through [`ScriptOffsetRequestGuard`], because it only ever sees one chunk at a time.
pub fn validate_script_offset_key_identities(
    sender_offset_indexes: &[(LedgerKeyBranch, u64)],
    script_key_indexes: &[(LedgerKeyBranch, u64)],
) -> Result<(), ScriptOffsetPolicyError> {
    let mut guard = ScriptOffsetRequestGuard::new();
    for (role, identities) in [
        (ScriptOffsetRole::SenderOffset, sender_offset_indexes),
        (ScriptOffsetRole::ScriptKey, script_key_indexes),
    ] {
        for (branch, index) in identities {
            guard.record_indexed_key(branch.as_byte(), *index, role)?;
        }
    }
    Ok(())
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
    /// Whether the spend key has been folded in by the derived-key path. It is not host-addressable, and however
    /// many derived keys a request carries it is still one key, so it counts once.
    alpha_used: bool,
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

    /// Record that the spend key has been folded in through the derived-key path.
    pub fn record_alpha(&mut self) {
        self.alpha_used = true;
    }

    /// How many distinct Ledger keys this request draws on.
    pub fn unique_key_count(&self) -> usize {
        self.key_identities.len().saturating_add(usize::from(self.alpha_used))
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
        validate_script_offset_key_identities,
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

    /// Replay a complete request through the guard exactly as the device handler drives it, and report whether the
    /// device would have returned an offset.
    fn run_request(
        sender: &[(LedgerKeyBranch, u64)],
        script: &[(LedgerKeyBranch, u64)],
        derived_sender: u64,
        derived_script: u64,
    ) -> Result<(), ScriptOffsetPolicyError> {
        let mut guard = ScriptOffsetRequestGuard::new();
        guard.begin(
            ScriptOffsetTotals {
                sender_offset_indexes: sender.len() as u64,
                script_key_indexes: script.len() as u64,
                derived_sender_offsets: derived_sender,
                derived_script_keys: derived_script,
            },
            MAX_CHUNK,
        )?;

        let last = 1 + sender.len() as u64 + script.len() as u64 + derived_sender + derived_script;
        let mut sender_keys = sender.iter();
        let mut script_keys = script.iter();
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
                ChunkKind::DerivedSenderOffset | ChunkKind::DerivedScriptKey => guard.record_alpha(),
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
            run_request(&[(LedgerKeyBranch::OneSidedSenderOffset, 7)], &[], 0, 1),
            Ok(())
        );
    }

    #[test]
    fn multi_input_transaction_is_accepted() {
        // A transaction with several inputs sends one derived script key per input. They all fold in the same spend
        // key, which must not be mistaken for the request leaning on a single key over and over.
        for inputs in 1..8 {
            assert_eq!(
                run_request(&[(LedgerKeyBranch::OneSidedSenderOffset, 1)], &[], 0, inputs),
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
        assert_eq!(run_request(&sender, &[], 0, 2), Ok(()));
    }

    #[test]
    fn pre_mine_spend_is_accepted() {
        assert_eq!(
            run_request(&[(LedgerKeyBranch::Random, 99)], &[(LedgerKeyBranch::PreMine, 3)], 0, 0),
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
            run_request(&sender, &script, 0, 0),
            Err(ScriptOffsetPolicyError::BranchNotAllowedForRole {
                branch: LedgerKeyBranch::Spend,
                role: ScriptOffsetRole::SenderOffset,
            })
        );
    }

    #[test]
    fn advisory_negative_control_still_rejected() {
        // The advisory's baseline: the target key on its own. Rejected before the fix too, and still rejected.
        assert!(run_request(&[(LedgerKeyBranch::Spend, STATIC_SPEND_INDEX)], &[], 0, 0).is_err());
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
        assert!(run_request(&[target], &[], 0, 0).is_err());
        assert!(run_request(&[], &[target], 0, 0).is_err());
        assert!(
            run_request(
                &[target, (LedgerKeyBranch::Random, 0)],
                &[(LedgerKeyBranch::PreMine, 1)],
                0,
                0
            )
            .is_err()
        );
        assert!(run_request(&[(LedgerKeyBranch::Random, 0), target], &[], 0, 2).is_err());
        assert!(run_request(&[(LedgerKeyBranch::Spend, 0)], &[], 0, 1).is_err());
    }

    #[test]
    fn ephemeral_nonce_branch_is_never_a_valid_operand() {
        for role in [ScriptOffsetRole::SenderOffset, ScriptOffsetRole::ScriptKey] {
            assert!(!branch_is_valid_for_role(LedgerKeyBranch::MetadataEphemeralNonce, role));
        }
        assert!(run_request(&[(LedgerKeyBranch::MetadataEphemeralNonce, 1)], &[], 0, 1).is_err());
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
            run_request(&[(LedgerKeyBranch::Random, 42)], &[], 0, 0),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 })
        );
        assert_eq!(
            run_request(&[], &[(LedgerKeyBranch::PreMine, 1)], 0, 0),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 })
        );
    }

    #[test]
    fn derived_keys_alone_are_one_key_however_many_there_are() {
        // Every derived key folds in the same spend key, so a request made only of derived keys leans on exactly one
        // Ledger secret no matter how long it is.
        for derived in 1..6 {
            assert_eq!(
                run_request(&[], &[], 0, derived),
                Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 }),
                "{derived} derived script keys still only draw on the spend key"
            );
        }
        assert_eq!(
            run_request(&[], &[], 2, 3),
            Err(ScriptOffsetPolicyError::NotEnoughUniqueKeys { unique: 1 })
        );
    }

    #[test]
    fn an_empty_request_is_rejected() {
        assert_eq!(
            run_request(&[], &[], 0, 0),
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
                0,
                0
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
            run_request(&[(LedgerKeyBranch::PreMine, 5)], &[(LedgerKeyBranch::PreMine, 5)], 0, 0),
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
                0,
                0
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
        guard.record_alpha();
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

    #[test]
    fn host_side_check_agrees_with_the_device_on_key_identities() {
        assert_eq!(
            validate_script_offset_key_identities(&[(LedgerKeyBranch::OneSidedSenderOffset, 7)], &[]),
            Ok(())
        );
        assert_eq!(
            validate_script_offset_key_identities(&[(LedgerKeyBranch::Random, 99)], &[(LedgerKeyBranch::PreMine, 3)]),
            Ok(())
        );
        assert!(
            validate_script_offset_key_identities(
                &[
                    (LedgerKeyBranch::Spend, STATIC_SPEND_INDEX),
                    (LedgerKeyBranch::Spend, 43)
                ],
                &[(LedgerKeyBranch::Spend, 43)],
            )
            .is_err()
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
