// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! ③ `ledger_get_raw_schnorr_signature_legacy_nonce_wrapper`, and the `(LedgerKey, LedgerKey)` arm of
//! `sign_with_nonce_and_challenge` that is its only caller.
//!
//! Read `minotari_ledger_wallet_common::legacy_nonce` first. It is the canonical account of this instruction and
//! is not restated here: the host names the nonce by branch and index, the device re-derives the same scalar every
//! time, and two signatures over one `(key, nonce)` pair with different challenges give up the key. It survives
//! for the pre-mine spend flow alone, whose nonces cross a session *file* rather than a device session, and the
//! branch whitelist in that module is the whole of its containment.
//!
//! So these tests say out loud what the arm does today, which is what makes deleting it safe when the TODO there
//! lands - at which point this module goes in the same commit:
//!
//! * the one whitelisted pair - a `PreMine` key against a `Random` nonce - reaches the device, is shown for approval,
//!   and signs, and the nonce **is** the `Random` branch key at the named index - deterministic, by construction, and
//!   visible as such;
//! * the pairs off the whitelist - a `OneSidedSenderOffset` key above all, which is the pair that reached `alpha` - are
//!   turned away by the key manager before the transport is opened (the device's own copy of the whitelist is the Spec
//!   4 `legacy_nonce` scenarios' job, over raw APDUs);
//! * and the legacy instruction does not touch the ephemeral nonce store the handle based path relies on.

use minotari_ledger_wallet_common::common_types::{Instruction, LedgerKeyBranch};
use minotari_ledger_wallet_comms_testing::{
    approver::{Outcome, while_reviewing},
    fixtures,
    review::ExpectedReview,
};
use tari_common_types::types::CompressedSignature;
use tari_transaction_components::key_manager::{
    KeyManager,
    TariKeyId,
    TransactionKeyManagerInterface,
    error::KeyManagerError,
};
use tari_utilities::hex::Hex;

use crate::harness::{Device, ledger_error, with_device};

fn ledger_key(branch: LedgerKeyBranch) -> TariKeyId {
    TariKeyId::LedgerKey {
        branch,
        index: fixtures::random_u64(),
    }
}

/// Sign through the `(LedgerKey, LedgerKey)` arm, approving the review the device puts up for it - after checking it
/// names the key and nonce that were asked for.
fn reviewed_legacy_signature(
    device: &Device,
    key_manager: &KeyManager,
    key_id: &TariKeyId,
    nonce_id: &TariKeyId,
    challenge: &[u8; 64],
) -> Result<CompressedSignature, KeyManagerError> {
    let (
        TariKeyId::LedgerKey {
            branch: key_branch,
            index: key_index,
        },
        TariKeyId::LedgerKey {
            branch: nonce_branch,
            index: nonce_index,
        },
    ) = (key_id, nonce_id)
    else {
        panic!("a legacy signature is a ledger key against a ledger nonce, got {key_id} and {nonce_id}");
    };
    let expected = ExpectedReview::legacy_signature(
        &key_branch.to_string(),
        *key_index,
        &nonce_branch.to_string(),
        *nonce_index,
    );
    let (signature, review) = while_reviewing(device.approver(), &expected, Outcome::Approve, || {
        key_manager.sign_with_nonce_and_challenge(key_id, nonce_id, challenge)
    });
    review.unwrap_or_else(|e| panic!("the device's review of {}: {e}", expected.summary()));
    signature
}

/// `signature` verifies against the key `key_id` names, as the key manager reports it.
fn assert_verifies(
    key_manager: &KeyManager,
    signature: &CompressedSignature,
    key_id: &TariKeyId,
    challenge: &[u8; 64],
) {
    let public_key = key_manager
        .get_public_key_at_key_id(key_id)
        .expect("the signing key's public key")
        .to_public_key()
        .expect("it decompresses");
    assert!(
        signature
            .to_schnorr_signature()
            .expect("the device's signature decompresses")
            .verify_raw_uniform(&public_key, challenge),
        "the legacy signature by {key_id} does not verify"
    );
}

/// The whitelisted pair signs through the legacy arm, after the user approves the device's review, and the nonce it
/// signs with is the `Random` branch key at the index the host named - the same one, every time it is asked.
///
/// `PreMine` keys sign both the pre-mine script signature and its metadata signature (with the `PreMine` sender
/// offset key `get_script_offset` issued in pre-mine mode). The second signature is over a different challenge with
/// the same pair, and its public nonce is identical: that equality *is* the cost the canonical module describes,
/// pinned so that nobody mistakes this arm for the handle based one.
///
/// Both signatures went out as `GetRawSchnorrSignatureLegacyNonce`, and nothing was reserved, which is what makes
/// this arm distinguishable from the other one on the wire as well as in the arithmetic.
#[test]
fn the_whitelisted_pair_signs_with_the_indexed_random_key_as_its_nonce() {
    with_device(|device| {
        let key_manager = device.key_manager();

        let key_id = ledger_key(LedgerKeyBranch::PreMine);
        let nonce_id = ledger_key(LedgerKeyBranch::Random);
        let expected_nonce = key_manager
            .get_public_key_at_key_id(&nonce_id)
            .expect("the Random branch key the nonce is derived as");

        let challenges = [fixtures::random_challenge(), fixtures::random_challenge()];
        let (signatures, wire) = device.watch(|| {
            challenges
                .iter()
                .map(|challenge| {
                    reviewed_legacy_signature(device, &key_manager, &key_id, &nonce_id, challenge)
                        .unwrap_or_else(|e| panic!("a legacy signature by a PreMine key: {e}"))
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(
            (
                wire.count(Instruction::GetRawSchnorrSignatureLegacyNonce),
                wire.count(Instruction::GenerateEphemeralNonce),
                wire.count(Instruction::GetRawSchnorrSignature)
            ),
            (2, 0, 0),
            "a (LedgerKey, LedgerKey) pair must take the legacy instruction and only it: {wire:?}"
        );

        for (signature, challenge) in signatures.iter().zip(&challenges) {
            assert_eq!(
                signature.get_compressed_public_nonce().to_hex(),
                expected_nonce.to_hex(),
                "the legacy nonce for a PreMine key should be the Random branch key at the named index"
            );
            assert_verifies(&key_manager, signature, &key_id, challenge);
        }
    });
}

/// Indexes that agree modulo `2^32` are different keys and different nonces, and the review tells them apart.
///
/// The device used to derive a key from its index modulo `2^32`, so `PreMine i` and `PreMine 2^63 | i` were one key,
/// and `Random j` and `Random j + 2^32` one nonce - while the review showed them as two different pre-mine signatures
/// ("script" then "metadata"), which a host could have approved twice and differenced for the key. The device now
/// derives from every bit of the index, so each request is answered against its *own* expected review - the full
/// `u64`s and the purpose read from bit 63, an exact field-text match - and the two sign with different keys and
/// different nonces: over one challenge, two different signatures, each verifying against its own key.
#[test]
fn indexes_equal_modulo_2_32_show_different_reviews_and_sign_with_different_keys() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let i = fixtures::random_u64() % 100_000;
        let j = fixtures::random_u64() & 0xFFFF_FFFF;
        let challenge = fixtures::random_challenge();

        let pairs = [(i, j), ((1u64 << 63) | i, j | (1u64 << 32))];
        let mut summaries = Vec::new();
        let mut signatures = Vec::new();
        let mut public_keys = Vec::new();
        for (key_index, nonce_index) in pairs {
            let key_id = TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::PreMine,
                index: key_index,
            };
            let nonce_id = TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::Random,
                index: nonce_index,
            };
            summaries.push(
                ExpectedReview::legacy_signature(
                    &LedgerKeyBranch::PreMine.to_string(),
                    key_index,
                    &LedgerKeyBranch::Random.to_string(),
                    nonce_index,
                )
                .summary(),
            );
            let signature = reviewed_legacy_signature(device, &key_manager, &key_id, &nonce_id, &challenge)
                .unwrap_or_else(|e| panic!("a legacy signature by PreMine {key_index}: {e}"));
            assert_verifies(&key_manager, &signature, &key_id, &challenge);
            public_keys.push(
                key_manager
                    .get_public_key_at_key_id(&key_id)
                    .expect("the signing key's public key")
                    .to_hex(),
            );
            signatures.push(signature);
        }

        assert_ne!(
            summaries[0], summaries[1],
            "the two requests must not look alike on the device"
        );
        assert_ne!(
            public_keys[0], public_keys[1],
            "PreMine {i} and PreMine 2^63 | {i} should be different keys"
        );
        assert_ne!(
            signatures[0].get_compressed_public_nonce().to_hex(),
            signatures[1].get_compressed_public_nonce().to_hex(),
            "Random {j} and Random {j} + 2^32 should be different nonces"
        );
        assert_ne!(
            signatures[0].get_signature().to_hex(),
            signatures[1].get_signature().to_hex(),
            "two different keys and nonces over one challenge should be two signatures"
        );
    });
}

/// The pairs off the whitelist are refused by the key manager, and the device is never asked.
///
/// `OneSidedSenderOffset` is the branch `get_script_offset` blinds `alpha` with, and its index comes back with the
/// reply: two legacy signatures under it gave up `alpha`. `Random` is not a branch pre-mine signs with. `Spend` is
/// `alpha` and is never signable by index - on this instruction least of all. A nonce off the `Random`
/// branch would turn the instruction into a way to extract a key on some other branch by pointing its nonce there.
/// A ledger key paired with a host key is not a legacy pair at all, and is refused by the dispatch before either
/// wrapper.
#[test]
fn pairs_off_the_legacy_whitelist_are_refused_before_the_device_is_asked() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let host_key = key_manager.get_random_key(None, None).expect("a host key").key_id;

        let refusals = [
            (
                "a OneSidedSenderOffset key",
                ledger_key(LedgerKeyBranch::OneSidedSenderOffset),
                ledger_key(LedgerKeyBranch::Random),
                "keys cannot be signed with a host chosen nonce",
            ),
            (
                "a Random key",
                ledger_key(LedgerKeyBranch::Random),
                ledger_key(LedgerKeyBranch::Random),
                "keys cannot be signed with a host chosen nonce",
            ),
            (
                "a Spend key",
                ledger_key(LedgerKeyBranch::Spend),
                ledger_key(LedgerKeyBranch::Random),
                "keys cannot be signed with a host chosen nonce",
            ),
            (
                "a PreMine nonce",
                ledger_key(LedgerKeyBranch::PreMine),
                ledger_key(LedgerKeyBranch::PreMine),
                "the nonce branch must be",
            ),
            (
                "a OneSidedSenderOffset nonce",
                ledger_key(LedgerKeyBranch::PreMine),
                ledger_key(LedgerKeyBranch::OneSidedSenderOffset),
                "the nonce branch must be",
            ),
            (
                "a host held nonce",
                ledger_key(LedgerKeyBranch::PreMine),
                host_key.clone(),
                "paired to a non ledger key",
            ),
            (
                "a host held key",
                host_key,
                ledger_key(LedgerKeyBranch::Random),
                "paired to a non ledger key",
            ),
        ];
        for (label, key_id, nonce_id, reason) in refusals {
            let (result, wire) = device
                .watch(|| key_manager.sign_with_nonce_and_challenge(&key_id, &nonce_id, &fixtures::random_challenge()));
            let error = ledger_error(result.expect_err(label));
            assert!(
                error.contains(reason),
                "{label} should be refused because '{reason}', got: {error}"
            );
            assert!(wire.is_empty(), "{label} reached the device: {wire:?}");
        }
    });
}

/// A legacy signature leaves the ephemeral nonce store alone: a handle reserved before it is still good after it.
///
/// The two instructions share a handler file on the device and a dispatch arm's worth of distance in the key
/// manager. A legacy path that consumed, cleared or reused a store slot would break whatever multi-party exchange
/// had a reservation outstanding at the time.
#[test]
fn the_legacy_instruction_leaves_the_nonce_store_alone() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let key_id = ledger_key(LedgerKeyBranch::PreMine);
        let reserved = key_manager.reserve_ephemeral_nonce().expect("GenerateEphemeralNonce");

        let challenge = fixtures::random_challenge();
        let legacy = reviewed_legacy_signature(
            device,
            &key_manager,
            &key_id,
            &ledger_key(LedgerKeyBranch::Random),
            &challenge,
        )
        .expect("a legacy signature while a reservation is outstanding");
        assert_verifies(&key_manager, &legacy, &key_id, &challenge);

        let challenge = fixtures::random_challenge();
        let signature = key_manager
            .sign_with_nonce_and_challenge(&key_id, &reserved.key_id, &challenge)
            .expect("the reservation made before the legacy signature");
        assert_eq!(
            signature.get_compressed_public_nonce().to_hex(),
            reserved.pub_key.to_hex(),
            "the handle no longer named the nonce it was reserved for"
        );
        assert_verifies(&key_manager, &signature, &key_id, &challenge);
    });
}
