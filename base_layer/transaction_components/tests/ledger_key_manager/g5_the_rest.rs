// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! ⑤ The remaining wrappers: `ledger_get_public_key_wrapper`, `ledger_get_script_signature_wrapper`,
//! `ledger_get_script_schnorr_signature_wrapper` and `ledger_get_dh_shared_secret_wrapper`.
//!
//! Lowest risk, not no risk. Each of these translates a key id into an instruction, and the translation is where
//! they can go wrong: a `Derived` script key has to go out as its blinding factor so that the device adds `alpha`,
//! a `PreMine` one as a branch and index, and a key the device cannot hold must be turned away rather than sent.
//! Every result is checked as the equation it has to satisfy, with the host side computed by
//! `transaction_components` itself - `TransactionInput::validate_script_signature`, the key manager's own public
//! keys - so a verifying result is a statement that the device and the rest of the wallet agree.
//!
//! The public key wrapper is additionally pinned to the Spec 2 vector table, row for row, through the key manager:
//! a ledger wallet bootstrapped on each row's account must report the key that row freezes.

use minotari_ledger_wallet_common::common_types::{Instruction, LedgerKeyBranch};
use minotari_ledger_wallet_comms_testing::{
    fixtures,
    vectors::{DERIVATION_VECTORS, DeviceCall, EXPECTED_VECTOR_COUNT},
};
use tari_common_types::types::{CommitmentFactory, CompressedPublicKey, PrivateKey};
use tari_crypto::keys::SecretKey;
use tari_transaction_components::{
    MicroMinotari,
    key_manager::{SecretTransactionKeyManagerInterface, TariKeyId, TransactionKeyManagerInterface},
    test_helpers::{TestParams, UtxoTestParams},
    transaction_components::{TransactionInput, TransactionInputVersion, one_sided::public_key_to_output_spending_key},
};
use tari_utilities::hex::Hex;

use crate::harness::{ledger_error, with_device};

/// Every row of the vector table, asked through a ledger-mode key manager on that row's account.
///
/// `GetPublicKey` rows go through `get_public_key_at_key_id` - the public key wrapper. The `GetPublicSpendKey` and
/// `GetViewKey` rows are the two values the bootstrap reads off the device, so they check that the key manager
/// holds exactly what the console wallet's init path would have handed it. The row count is pinned first, because
/// a loop over a truncated table would pass having checked nothing.
#[test]
fn every_derivation_vector_is_what_the_key_manager_reports() {
    with_device(|device| {
        assert_eq!(
            DERIVATION_VECTORS.len(),
            EXPECTED_VECTOR_COUNT,
            "the vector table was truncated"
        );

        let mut failures = Vec::new();
        for vector in DERIVATION_VECTORS {
            let key_manager = device.key_manager_for(vector.account);
            let actual = match vector.call {
                DeviceCall::PublicKey { index, branch } => key_manager
                    .get_public_key_at_key_id(&TariKeyId::LedgerKey { branch, index })
                    .unwrap_or_else(|e| panic!("'{}': {e}", vector.name))
                    .to_hex(),
                DeviceCall::PublicSpendKey => key_manager.get_spend_key().pub_key.to_hex(),
                DeviceCall::ViewKey => key_manager.get_private_view_key().to_hex(),
            };
            let expected = vector.expected(device.seed());
            if actual != expected {
                failures.push(format!("  '{}': expected {expected}, got {actual}", vector.name));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} vectors disagree with the key manager under the '{}' seed:\n{}",
            failures.len(),
            DERIVATION_VECTORS.len(),
            device.seed().name(),
            failures.join("\n")
        );
    });
}

/// A device key the key manager hands out is the device's key at the index it recorded, and the spend branch is
/// answered from the bootstrap without asking the device at all.
///
/// `get_random_key` picks a random index and asks the device for that index's public key once; asking again for the
/// same key id must give the same point, or the key id does not name what the caller was told it names. The spend
/// branch is never addressable by index on the device, so a wrapper that sent it would be refused - the key
/// manager answers it with `alpha·G` from `GetPublicSpendKey` instead.
#[test]
fn a_device_key_id_names_the_same_public_key_every_time() {
    with_device(|device| {
        let key_manager = device.key_manager();

        for branch in [LedgerKeyBranch::Random, LedgerKeyBranch::PreMine] {
            let (issued, wire) = device.watch(|| key_manager.get_random_key(None, Some(branch)).expect("a device key"));
            assert_eq!(wire.count(Instruction::GetPublicKey), 1, "{wire:?}");
            let again = key_manager
                .get_public_key_at_key_id(&issued.key_id)
                .expect("the same key id, asked again");
            assert_eq!(
                again, issued.pub_key,
                "a '{branch}' key id named two different public keys"
            );
        }

        let spend = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::Spend,
            index: fixtures::random_u64(),
        };
        let (public_spend_key, wire) =
            device.watch(|| key_manager.get_public_key_at_key_id(&spend).expect("the spend branch"));
        assert_eq!(public_spend_key, key_manager.get_spend_key().pub_key);
        assert!(wire.is_empty(), "the spend branch reached the device: {wire:?}");
    });
}

/// An output built through `crate::test_helpers` spends as a transaction input whose script signature verifies.
///
/// This is the ordinary wallet spend: the script key is `Derived` from the output's commitment mask, so the wrapper
/// sends the mask as a blinding factor and the device signs with `H(mask) + alpha` - a key only the device can
/// produce. `TransactionInput::validate_script_signature` then checks it against the script public key the host
/// computed from `alpha·G`, with the challenge the host builds under its own network. One `GetScriptSignatureDerived`
/// on the wire and nothing else signing.
#[test]
fn a_derived_script_key_signs_a_transaction_input_that_verifies() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let mut params = TestParams::new(&key_manager);
        // The output being spent is only the fixture here. Its metadata signature is signed with no recipient address,
        // which a ledger wallet refuses for a device held sender offset key, so the fixture carries a host held one.
        let host_sender_offset = key_manager.get_random_key(None, None).expect("a host key");
        params.sender_offset_key_id = host_sender_offset.key_id;
        params.sender_offset_key_pk = host_sender_offset.pub_key;
        let output = params.create_input(UtxoTestParams::with_value(MicroMinotari(7_000)), &key_manager);

        let (input, wire) = device.watch(|| output.to_transaction_input(&key_manager).expect("a transaction input"));
        assert_eq!(
            (
                wire.count(Instruction::GetScriptSignatureDerived),
                wire.count(Instruction::GetScriptSignatureManaged)
            ),
            (1, 0),
            "a derived script key must be signed with GetScriptSignatureDerived: {wire:?}"
        );
        input
            .validate_script_signature(&params.script_key_pk, &CommitmentFactory::default())
            .expect("the device's script signature verifies as the input's");
    });
}

/// A pre-mine script key signs by branch and index, and the signature verifies against the device's key at that
/// index; a key the device cannot hold is refused by the key manager before the wire.
///
/// The accepting half takes the `Managed` arm of the wrapper - `GetScriptSignatureManaged` - which the ordinary spend
/// above never reaches. The refusing half is a host held key offered as a script key on a ledger wallet: the wrapper
/// has no instruction to send it with, and must say so rather than send something else.
#[test]
fn a_pre_mine_script_key_signs_by_index_and_a_host_key_is_refused() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let script_key = key_manager
            .get_random_key(None, Some(LedgerKeyBranch::PreMine))
            .expect("a pre-mine key");
        let commitment_mask = key_manager.get_random_key(None, None).expect("a commitment mask");
        let value = PrivateKey::from(9_000u64);
        let message = fixtures::random_bytes_32();
        let version = TransactionInputVersion::get_current_version();

        let (signature, wire) = device.watch(|| {
            key_manager
                .get_script_signature(&script_key.key_id, &commitment_mask.key_id, &value, version, &message)
                .expect("a managed script signature")
        });
        assert_eq!(wire.count(Instruction::GetScriptSignatureManaged), 1, "{wire:?}");

        let commitment = key_manager
            .get_commitment(&commitment_mask.key_id, &value)
            .expect("the commitment");
        let challenge = TransactionInput::finalize_script_signature_challenge(
            version,
            signature.ephemeral_commitment(),
            signature.ephemeral_pubkey(),
            &script_key.pub_key,
            &commitment,
            &message,
        );
        assert!(
            signature
                .to_capk_signature()
                .expect("the device's signature decompresses")
                .verify_challenge(
                    &commitment.to_commitment().expect("the commitment decompresses"),
                    &script_key.pub_key.to_public_key().expect("the script key decompresses"),
                    &challenge,
                    &CommitmentFactory::default(),
                    &mut rand::rng(),
                ),
            "the managed script signature does not verify against the device's pre-mine key"
        );

        let host_key = key_manager.get_random_key(None, None).expect("a host key");
        let (result, wire) = device.watch(|| {
            key_manager.get_script_signature(&host_key.key_id, &commitment_mask.key_id, &value, version, &message)
        });
        let error = result
            .expect_err("a host held script key on a ledger wallet")
            .to_string();
        assert!(error.contains("Ledger does not support"), "{error}");
        assert!(wire.is_empty(), "a host held script key reached the device: {wire:?}");
    });
}

/// A script Schnorr signature verifies against the device key it names, with a fresh nonce each time; the spend
/// branch is refused by the device.
///
/// This wrapper has no host side mirror of the device's branch rule, so the spend branch refusal is the device's
/// `BadBranchKey`, seen through the key manager - which is the answer a caller gets, and so the one worth pinning.
#[test]
fn a_script_schnorr_signature_verifies_against_the_key_it_names() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
        let (_offset, sender_offsets) = key_manager
            .get_script_offset(std::slice::from_ref(&script_key.key_id), 1)
            .expect("a sender offset key");

        let mut keys = sender_offsets;
        for branch in [LedgerKeyBranch::Random, LedgerKeyBranch::PreMine] {
            keys.push(key_manager.get_random_key(None, Some(branch)).expect("a device key"));
        }
        for key in keys {
            let message = fixtures::random_bytes_32();
            let public_key = key.pub_key.to_public_key().expect("the key decompresses");
            let first = key_manager
                .sign_script_message(&key.key_id, &message)
                .unwrap_or_else(|e| panic!("a script Schnorr signature by {}: {e}", key.key_id));
            let second = key_manager
                .sign_script_message(&key.key_id, &message)
                .unwrap_or_else(|e| panic!("a second script Schnorr signature by {}: {e}", key.key_id));
            assert_ne!(
                first, second,
                "two signatures by {} over one message are identical, so the device reused its nonce",
                key.key_id
            );
            for signature in [first, second] {
                assert!(
                    signature
                        .to_schnorr_signature()
                        .expect("the device's signature decompresses")
                        .verify(&public_key, message),
                    "the script Schnorr signature by {} does not verify",
                    key.key_id
                );
            }
        }

        let spend = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::Spend,
            index: fixtures::random_u64(),
        };
        let error = ledger_error(
            key_manager
                .sign_script_message(&spend, &fixtures::random_bytes_32())
                .expect_err("a script Schnorr signature by the spend branch"),
        );
        assert!(error.contains("BadBranchKey"), "{error}");
    });
}

/// The Diffie-Hellman secret a device held sender offset key produces is the one the receiver computes from its own
/// side, and the one-sided commitment mask the wallet derives from it agrees.
///
/// This is the use the wrapper has: a one-sided sender computes `k·V` on the device from its sender offset key and
/// the receiver's view key, and the receiver later computes `v·K` from the output's sender offset public key. They
/// have to meet, or the receiver never finds the output. The second assertion goes one step further, through
/// `TariKeyId::DHCommitmentMask` - the key id a one-sided output's commitment mask actually is - so the wrapper is
/// checked in the place the key manager consumes it.
#[test]
fn a_diffie_hellman_secret_through_a_device_key_meets_the_receivers() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
        let (_offset, mut sender_offsets) = key_manager
            .get_script_offset(std::slice::from_ref(&script_key.key_id), 1)
            .expect("a sender offset key");
        let sender_offset = sender_offsets.pop().expect("one sender offset key");

        let receiver_view_key = PrivateKey::random(&mut rand::rng());
        let receiver_public_view_key = CompressedPublicKey::from_secret_key(&receiver_view_key);

        let (shared, wire) = device.watch(|| {
            key_manager
                .get_diffie_hellman_shared_secret(&sender_offset.key_id, &receiver_public_view_key)
                .expect("GetDHSharedSecret")
        });
        assert_eq!(wire.count(Instruction::GetDHSharedSecret), 1, "{wire:?}");

        let receivers = CompressedPublicKey::new_from_pk(
            &receiver_view_key *
                &sender_offset
                    .pub_key
                    .to_public_key()
                    .expect("the sender offset key decompresses"),
        );
        assert_eq!(
            shared.to_hex(),
            receivers.to_hex(),
            "the sender's k·V on the device is not the receiver's v·K"
        );

        let commitment_mask = key_manager
            .get_private_key(&TariKeyId::DHCommitmentMask {
                public_key: receiver_public_view_key,
                private_key: sender_offset.key_id.clone().into(),
            })
            .expect("the one-sided commitment mask");
        assert_eq!(
            commitment_mask,
            public_key_to_output_spending_key(&receivers).expect("the receiver's commitment mask"),
            "the sender's one-sided commitment mask is not the one the receiver derives"
        );
    });
}
