// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! ① `ledger_get_script_offset_wrapper`, through `get_script_offset`.
//!
//! First because it is the one wrapper where a mistake is a key disclosure rather than a failed transaction. The
//! script offset is `Σ script keys − Σ sender offset keys`, and each side blinds the other:
//!
//! * with no sender offset key, the reply is the plain sum of the input script keys - each `H(b) + alpha` for a `b` the
//!   host chose - and subtracting the hashes it knows leaves `alpha`, **the wallet's spend key**;
//! * with no device derived script key, the reply is `−k_sender` for a key the device just generated, which is exactly
//!   the sender offset private key the device exists to keep from the host.
//!
//! The device refuses both (`check_offset_is_blinded` and friends in `common/src/script_offset.rs`), and
//! `manager.rs` mirrors both so that a request the device would refuse never reaches the wire. Those refusals are
//! asserted here *as refusals the device was never asked about*: the wire is watched and must stay empty. That the
//! device's own copy of each rule holds is the Spec 4 `stateful` scenarios' job, over raw APDUs.
//!
//! The request is also **chunked and stateful**: one APDU per script key, accumulated on the device across
//! exchanges, and voided by anything that lands between two of them. The key manager is `Clone` and shared, so
//! "anything" includes another caller's instruction - which the interleaving test puts there on purpose.
//!
//! Every value that comes back is checked as an equation in the group, `offset·G = ΣK_script − ΣK_sender`, with
//! the public keys taken from the key manager itself: the host computes the alpha derived ones from `alpha·G`, and
//! asks the device for the pre-mine and sender offset ones.

use std::slice;

use minotari_ledger_wallet_common::{
    common_types::{Instruction, LedgerKeyBranch},
    script_offset::{MAX_SENDER_OFFSET_KEYS, sender_offset_index},
};
use tari_common_types::types::PrivateKey;
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
use tari_transaction_components::key_manager::{
    KeyManager,
    TariKeyAndId,
    TariKeyId,
    TransactionKeyManagerInterface,
    error::KeyManagerError,
};
use tari_utilities::hex::Hex;

use crate::harness::{Wire, ledger_error, with_device};

/// Script keys of all three kinds the wrapper sorts, deliberately interleaved so that the sorting is exercised
/// rather than the input already being in wire order:
///
/// * `Derived` - an alpha derived script key, sent as its blinding factor and derived on the device;
/// * `LedgerKey { PreMine, .. }` - a pre-mine script key, sent as an index and derived on the device;
/// * anything else - a host known key, folded into the one `partial_script_key_sum` scalar.
struct ScriptKeys {
    keys: Vec<TariKeyAndId>,
    pre_mine: usize,
    derived: usize,
}

impl ScriptKeys {
    fn new(key_manager: &KeyManager, pre_mine: usize, derived: usize, host_known: usize) -> Self {
        let mut pre_mine_keys: Vec<TariKeyAndId> = (0..pre_mine)
            .map(|_| {
                key_manager
                    .get_random_key(None, Some(LedgerKeyBranch::PreMine))
                    .unwrap()
            })
            .collect();
        let mut derived_keys: Vec<TariKeyAndId> = (0..derived)
            .map(|_| key_manager.get_next_commitment_mask_and_script_key().unwrap().1)
            .collect();
        let mut host_keys: Vec<TariKeyAndId> = (0..host_known)
            .map(|_| key_manager.get_random_key(None, None).unwrap())
            .collect();

        let mut keys = Vec::with_capacity(pre_mine.saturating_add(derived).saturating_add(host_known));
        while !(pre_mine_keys.is_empty() && derived_keys.is_empty() && host_keys.is_empty()) {
            keys.extend(host_keys.pop());
            keys.extend(derived_keys.pop());
            keys.extend(pre_mine_keys.pop());
        }
        Self {
            keys,
            pre_mine,
            derived,
        }
    }

    fn ids(&self) -> Vec<TariKeyId> {
        self.keys.iter().map(|key| key.key_id.clone()).collect()
    }

    /// The header, the partial sum, then one chunk per device derived key. Host known keys cost no chunk.
    fn expected_chunks(&self) -> usize {
        self.pre_mine.saturating_add(self.derived).saturating_add(2)
    }
}

/// `offset·G = ΣK_script − ΣK_sender`, or a failure that says which side of it disagreed.
// Ristretto point arithmetic, not integer arithmetic: these operators cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
fn assert_offset_is_the_difference(offset: &PrivateKey, script_keys: &[TariKeyAndId], sender_offsets: &[TariKeyAndId]) {
    let sum = |keys: &[TariKeyAndId]| {
        keys.iter().fold(RistrettoPublicKey::default(), |sum, key| {
            sum + key
                .pub_key
                .to_public_key()
                .expect("a key manager public key decompresses")
        })
    };
    let expected = sum(script_keys) - sum(sender_offsets);
    let actual = RistrettoPublicKey::from_secret_key(offset);
    assert_eq!(
        actual.to_hex(),
        expected.to_hex(),
        "the script offset is not Σ script keys − Σ sender offset keys ({} script keys, {} sender offset keys)",
        script_keys.len(),
        sender_offsets.len()
    );
}

/// Every sender offset key the device issued is on the sender offset branch, and they are the consecutive walk
/// `base..base + count` that the reply's single base index names - which is what the wrapper reconstructs them from.
fn assert_sender_offsets_are_one_walk(sender_offsets: &[TariKeyAndId], count: usize) {
    assert_eq!(sender_offsets.len(), count, "asked for {count} sender offset keys");
    let indexes: Vec<u64> = sender_offsets
        .iter()
        .map(|key| match key.key_id {
            TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::OneSidedSenderOffset,
                index,
            } => index,
            ref other => panic!("a sender offset key must be a device held OneSidedSenderOffset key, got {other}"),
        })
        .collect();
    let base = *indexes.first().expect("at least one sender offset key");
    for (i, index) in indexes.iter().enumerate() {
        assert_eq!(
            *index,
            sender_offset_index(base, i as u64),
            "sender offset key {i} is not step {i} of the walk from the base index: {indexes:?}"
        );
    }
    let distinct: std::collections::HashSet<String> = sender_offsets.iter().map(|key| key.pub_key.to_hex()).collect();
    assert_eq!(
        distinct.len(),
        count,
        "the device issued the same sender offset key twice"
    );
}

/// The wire shape of one `get_script_offset`: chunks `0..n` in order, every one but the last marked "more follows",
/// and every one accepted.
fn assert_one_clean_accumulation(wire: &Wire, expected_chunks: usize) {
    let chunks = wire.of(Instruction::GetScriptOffset);
    let numbers: Vec<u8> = chunks.iter().map(|chunk| chunk.p1).collect();
    let expected: Vec<u8> = (0..expected_chunks).map(|n| u8::try_from(n).unwrap()).collect();
    assert_eq!(
        numbers, expected,
        "the script offset did not go out as one in-order chunk sequence"
    );
    for (i, chunk) in chunks.iter().enumerate() {
        let last = i.saturating_add(1) == chunks.len();
        assert_eq!(chunk.p2, u8::from(!last), "chunk {i} has the wrong 'more follows' flag");
        assert_eq!(chunk.status, Some(0x9000), "chunk {i} was refused: {chunk:?}");
    }
}

/// The chunked path end to end: pre-mine, alpha derived and host known script keys in one call, several sender
/// offset keys back, and the offset is exactly the difference of the two sums.
///
/// Two pre-mine keys and three alpha derived ones make seven chunks. The two host known keys make none - they
/// travel inside the partial sum - which the chunk count pins, so a wrapper that started sending them as derived
/// keys (and so had the device fold `H(k) + alpha` instead of `k`) would fail here on the wire before it failed on
/// the arithmetic.
#[test]
fn a_mixed_chunked_script_offset_is_the_script_keys_minus_the_sender_offset_keys() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let script_keys = ScriptKeys::new(&key_manager, 2, 3, 2);
        let sender_offset_count = 3;

        let (result, wire) = device.watch(|| key_manager.get_script_offset(&script_keys.ids(), sender_offset_count));
        let (offset, sender_offsets) = result.expect("a mixed, blinded script offset");

        assert_one_clean_accumulation(&wire, script_keys.expected_chunks());
        assert_sender_offsets_are_one_walk(&sender_offsets, sender_offset_count);
        assert_offset_is_the_difference(&offset, &script_keys.keys, &sender_offsets);
    });
}

/// `MAX_SENDER_OFFSET_KEYS` is served, and one more is refused by the key manager before a single exchange.
///
/// The bound exists because the device performs every derivation in one exchange; the key manager checks it first
/// so that an over-large request surfaces as a typed error rather than a status word. Both halves, at the boundary.
#[test]
fn the_largest_sender_offset_count_is_served_and_one_more_is_refused_before_the_wire() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let script_keys = ScriptKeys::new(&key_manager, 0, 1, 0);
        let max = usize::try_from(MAX_SENDER_OFFSET_KEYS).unwrap();

        let (offset, sender_offsets) = key_manager
            .get_script_offset(&script_keys.ids(), max)
            .expect("the device derives up to MAX_SENDER_OFFSET_KEYS sender offset keys in one exchange");
        assert_sender_offsets_are_one_walk(&sender_offsets, max);
        assert_offset_is_the_difference(&offset, &script_keys.keys, &sender_offsets);

        let (result, wire) = device.watch(|| key_manager.get_script_offset(&script_keys.ids(), max + 1));
        assert_eq!(result.unwrap_err(), KeyManagerError::TooManySenderOffsetKeys {
            requested: max + 1,
            max,
        });
        assert!(wire.is_empty(), "an over-large request reached the device: {wire:?}");
    });
}

/// An unrelated instruction landing between two chunks of one `get_script_offset` voids the accumulation, and the
/// key manager reports a refusal - never a value.
///
/// This is the device's `offset_ctx.reset()` for "anything that is not the next chunk", seen from the only place it
/// matters to: a key manager whose call was cut into. That is not hypothetical. The key manager is `Clone`, the
/// wrappers hand clones out freely, and nothing holds the transport across a chunk sequence, so a second caller's
/// instruction can land mid-accumulation in a real wallet. What the device then owes is a refusal on the
/// terminating chunk, because the context the remaining chunks arrive into declares no sender offset keys at all.
///
/// The interruption is a real key manager call - a public key lookup on the same account - put in front of every
/// chunk after the header in turn, so each boundary is cut once. A positive control runs first and last: the same
/// request uninterrupted succeeds, and still succeeds after four voided attempts, so nothing leaked from one
/// accumulation into the next.
#[test]
fn an_instruction_interleaved_between_chunks_voids_the_script_offset() {
    with_device(|device| {
        let key_manager = device.key_manager();
        // Header, partial sum, one pre-mine key, two derived keys: chunks 0 to 4.
        let script_keys = ScriptKeys::new(&key_manager, 1, 2, 1);
        let chunks = script_keys.expected_chunks();

        let (control, wire) = device.watch(|| key_manager.get_script_offset(&script_keys.ids(), 1));
        let (offset, sender_offsets) = control.expect("the uninterrupted control");
        assert_one_clean_accumulation(&wire, chunks);
        assert_offset_is_the_difference(&offset, &script_keys.keys, &sender_offsets);

        for chunk in 1..u8::try_from(chunks).unwrap() {
            let other_caller = key_manager.clone();
            let (result, wire, fired) = device.interleave(
                Instruction::GetScriptOffset,
                chunk,
                move || {
                    other_caller
                        .get_random_key(None, Some(LedgerKeyBranch::Random))
                        .expect("the interleaved GetPublicKey");
                },
                || key_manager.get_script_offset(&script_keys.ids(), 1),
            );
            assert!(fired, "nothing was interleaved before chunk {chunk}");

            // The interruption really did land between chunk - 1 and chunk.
            let position = |ins: Instruction, p1: u8| wire.0.iter().position(|e| e.is(ins) && e.p1 == p1);
            let before = position(Instruction::GetScriptOffset, chunk - 1).expect("the chunk before the cut");
            let after = position(Instruction::GetScriptOffset, chunk).expect("the chunk after the cut");
            assert!(
                wire.0
                    .get(before + 1..after)
                    .is_some_and(|between| between.iter().any(|e| e.is(Instruction::GetPublicKey))),
                "the interleaved GetPublicKey did not land between chunks {} and {chunk}: {wire:?}",
                chunk - 1
            );

            let error = ledger_error(result.expect_err(
                "a script offset whose accumulation was cut into must be refused, not answered with a value",
            ));
            assert!(
                error.contains("ScriptOffsetNoSenderOffsets"),
                "interleaving before chunk {chunk} should leave a context with no sender offset keys, and the \
                 terminating chunk refused as ScriptOffsetNoSenderOffsets; got: {error}"
            );
        }

        let (offset, sender_offsets) = key_manager
            .get_script_offset(&script_keys.ids(), 1)
            .expect("the same request, uninterrupted, after the voided ones");
        assert_offset_is_the_difference(&offset, &script_keys.keys, &sender_offsets);
    });
}

/// Both unblinded shapes are refused by the key manager, and neither reaches the device.
///
/// * **No sender offset keys** - the reply would be `Σ(H(b) + alpha)`, and the host knows every `H(b)`.
/// * **No device derived script key** - the reply would be `−k_sender` for a key the device just made. A host known key
///   alone is refused, and so is `SpendKey`, which on a ledger wallet is not even a key the host can read.
/// * **No script keys at all** - the same leak as the line above, by the shortest route.
///
/// Each is paired with a control that differs from it only in the missing term and succeeds, so these cannot pass
/// against a key manager that has simply stopped serving script offsets.
#[test]
fn both_unblinded_script_offsets_are_refused_before_the_device_is_asked() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let derived = ScriptKeys::new(&key_manager, 0, 1, 0);
        let host_known = ScriptKeys::new(&key_manager, 0, 0, 2);

        let refusals: [(&str, Vec<TariKeyId>, usize, KeyManagerError); 5] = [
            (
                "no sender offset keys",
                derived.ids(),
                0,
                KeyManagerError::UnblindedScriptOffset {
                    script_keys: 1,
                    sender_offset_keys: 0,
                },
            ),
            (
                "no script keys at all",
                Vec::new(),
                1,
                KeyManagerError::UnblindedScriptOffset {
                    script_keys: 0,
                    sender_offset_keys: 1,
                },
            ),
            (
                "one host known script key",
                host_known.ids().into_iter().take(1).collect(),
                1,
                KeyManagerError::NoDeviceScriptKeys { script_keys: 1 },
            ),
            (
                "two host known script keys",
                host_known.ids(),
                1,
                KeyManagerError::NoDeviceScriptKeys { script_keys: 2 },
            ),
            (
                "the spend key as the only script key",
                vec![TariKeyId::SpendKey],
                1,
                KeyManagerError::NoDeviceScriptKeys { script_keys: 1 },
            ),
        ];
        for (label, script_key_ids, sender_offset_count, expected) in refusals {
            let (result, wire) = device.watch(|| key_manager.get_script_offset(&script_key_ids, sender_offset_count));
            assert_eq!(
                result.map(|(offset, _)| offset.to_hex()).unwrap_err(),
                expected,
                "a script offset with {label}"
            );
            assert!(
                wire.is_empty(),
                "a script offset with {label} reached the device: {wire:?}"
            );
        }

        // The controls: add back the one missing term and each request is served.
        let (offset, sender_offsets) = key_manager
            .get_script_offset(&derived.ids(), 1)
            .expect("the same derived key, with a sender offset key");
        assert_offset_is_the_difference(&offset, &derived.keys, &sender_offsets);

        let mut blinded = host_known.keys.clone();
        blinded.extend(derived.keys.iter().cloned());
        let blinded_ids: Vec<TariKeyId> = blinded.iter().map(|key| key.key_id.clone()).collect();
        let (offset, sender_offsets) = key_manager
            .get_script_offset(&blinded_ids, 1)
            .expect("the same host known keys, with one device derived key added");
        assert_offset_is_the_difference(&offset, &blinded, &sender_offsets);

        // And the single-key control really is one chunk per device key, not something the refusals above
        // short-circuited around.
        let derived_key = &derived.keys.first().expect("one derived key").key_id;
        let (result, wire) = device.watch(|| key_manager.get_script_offset(slice::from_ref(derived_key), 1));
        result.expect("the control, watched");
        assert_one_clean_accumulation(&wire, derived.expected_chunks());
    });
}
