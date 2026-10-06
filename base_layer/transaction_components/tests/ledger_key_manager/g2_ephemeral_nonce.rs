// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! ② `ledger_generate_ephemeral_nonce_wrapper` and `ledger_get_raw_schnorr_signature_wrapper`: reserve, then sign.
//!
//! The area of the last two fixes. A Schnorr signature gives up its key the moment one nonce signs two different
//! challenges, so on a ledger wallet the nonce is drawn by the device, kept by the device, and named to the host
//! only by an opaque handle that is good for exactly one signature. The key manager surfaces that as
//! `reserve_ephemeral_nonce` - which returns a `TariKeyId::LedgerEphemeralNonce` - and `sign_with_nonce_and_challenge`,
//! whose `(LedgerKey, LedgerEphemeralNonce)` arm spends it.
//!
//! Read `common/src/ephemeral_nonce.rs` before changing anything here. The store is eight slots and it **evicts**:
//! the ninth outstanding reservation pushes out the oldest, which is then refused as `NonceHandleInvalid`.
//! `NonceStoreFull` means the handle *counter* is exhausted - 2^64 reservations - and seeing it here would be a
//! bug, not the boundary.
//!
//! The last test is the fix itself (d60b8d8f0). `get_metadata_signature` - the path change outputs take - must sign
//! a device held sender offset key with a device reserved nonce, and must *not* reserve one for a host held sender
//! offset key, which is what coinbase and burn outputs still carry. Both halves are asserted on the wire.

use minotari_ledger_wallet_common::{
    common_types::{Instruction, LedgerKeyBranch},
    ephemeral_nonce::{EPHEMERAL_NONCE_STORE_SIZE, INVALID_NONCE_HANDLE},
};
use minotari_ledger_wallet_comms_testing::fixtures;
use tari_common_types::types::{CompressedPublicKey, CompressedSignature, PrivateKey};
use tari_crypto::keys::SecretKey;
use tari_script::{ExecutionStack, script};
use tari_transaction_components::{
    MicroMinotari,
    key_manager::{KeyManager, TariKeyAndId, TariKeyId, TransactionKeyManagerInterface},
    test_helpers::{TestParams, UtxoTestParams},
    transaction_components::WalletOutputBuilder,
};
use tari_utilities::hex::Hex;

use crate::harness::{ledger_error, with_device};

/// The handle a reservation carries, or a failure if the key manager returned anything but a device handle.
fn handle(nonce: &TariKeyAndId) -> u64 {
    match nonce.key_id {
        TariKeyId::LedgerEphemeralNonce { handle } => handle,
        ref other => panic!("a ledger wallet must reserve a device nonce handle, got {other}"),
    }
}

/// A device held sender offset key, issued the only way a ledger wallet issues one: by `get_script_offset`.
fn device_sender_offset_key(key_manager: &KeyManager) -> TariKeyAndId {
    let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
    let (_offset, mut sender_offsets) = key_manager
        .get_script_offset(std::slice::from_ref(&script_key.key_id), 1)
        .expect("a sender offset key from the device");
    sender_offsets.pop().expect("one sender offset key")
}

/// Assert `signature` is `key`'s signature over `challenge`, made with the nonce `nonce` names.
///
/// Both halves. `s·G = R + e·P` only holds for the `R` the reservation handed back and the `P` the key names, so a
/// device that signed with a nonce other than the reserved one - or quietly drew its own - fails here instead of
/// producing a signature that would not aggregate.
fn assert_signed_with(
    signature: &CompressedSignature,
    key: &CompressedPublicKey,
    nonce: &TariKeyAndId,
    challenge: &[u8; 64],
) {
    assert_eq!(
        signature.get_compressed_public_nonce().to_hex(),
        nonce.pub_key.to_hex(),
        "the device signed with some nonce other than the one handle {} named",
        handle(nonce)
    );
    let signature = signature
        .to_schnorr_signature()
        .expect("the device's signature decompresses");
    assert!(
        signature.verify_raw_uniform(&key.to_public_key().expect("the key decompresses"), challenge),
        "the signature made with nonce handle {} does not verify",
        handle(nonce)
    );
}

/// A handle reserved by one key manager call is spent by a later one, with a cross section of unrelated work in
/// between - which is the whole point of the store, and how every multi-party signing flow uses it.
///
/// The work in between is chosen to be what is most likely to disturb it: a whole `GetScriptOffset` accumulation
/// (the one other piece of device state, with its own reset), a public key, a Diffie-Hellman secret, a script
/// Schnorr signature, a *second* reservation spent inside the window, and a signature the device **refuses**, whose
/// error path runs the main loop's reset. None of it may touch the first handle.
#[test]
fn a_nonce_reserved_in_one_call_is_spent_in_a_later_one_across_unrelated_calls() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let sender_offset = device_sender_offset_key(&key_manager);
        let challenge = fixtures::random_challenge();

        let reserved = key_manager.reserve_ephemeral_nonce().expect("GenerateEphemeralNonce");

        // A whole script offset accumulation, start to finish.
        let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
        key_manager
            .get_script_offset(std::slice::from_ref(&script_key.key_id), 2)
            .expect("the interleaved script offset");
        // A public key and a Diffie-Hellman secret.
        let random_key = key_manager
            .get_random_key(None, Some(LedgerKeyBranch::Random))
            .expect("the interleaved GetPublicKey");
        let point = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()));
        key_manager
            .get_diffie_hellman_shared_secret(&sender_offset.key_id, &point)
            .expect("the interleaved GetDHSharedSecret");
        // A script Schnorr signature.
        key_manager
            .sign_script_message(&random_key.key_id, &fixtures::random_bytes_32())
            .expect("the interleaved GetScriptSchnorrSignature");
        // A second reservation, reserved and spent entirely inside the first one's window.
        let inner = key_manager.reserve_ephemeral_nonce().expect("the inner reservation");
        let inner_challenge = fixtures::random_challenge();
        let inner_signature = key_manager
            .sign_with_nonce_and_challenge(&random_key.key_id, &inner.key_id, &inner_challenge)
            .expect("the inner reservation's signature");
        assert_signed_with(&inner_signature, &random_key.pub_key, &inner, &inner_challenge);
        // And one the device refuses: a handle it never issued.
        let never_issued = TariKeyId::LedgerEphemeralNonce {
            handle: handle(&inner).saturating_add(1_000_000),
        };
        let refused = ledger_error(
            key_manager
                .sign_with_nonce_and_challenge(&random_key.key_id, &never_issued, &fixtures::random_challenge())
                .expect_err("a handle the device never issued"),
        );
        assert!(refused.contains("NonceHandleInvalid"), "{refused}");

        // Only now, the handle reserved before all of that.
        let signature = key_manager
            .sign_with_nonce_and_challenge(&sender_offset.key_id, &reserved.key_id, &challenge)
            .expect("a signature with a nonce reserved before several unrelated calls");
        assert_signed_with(&signature, &sender_offset.pub_key, &reserved, &challenge);
    });
}

/// A handle is good for exactly one signature, whichever clone of the key manager tries to spend it again.
///
/// On a software wallet a clone shares the store through an `Arc`; on a ledger wallet the store is the device, so
/// the clone cannot be a way round it either. The second attempt is refused as `NonceHandleInvalid` and produces no
/// signature - which is the disclosure the handle exists to close, since two signatures over one nonce give up the
/// key. Handle zero, the device's "no handle" value, names nothing at all.
#[test]
fn a_nonce_handle_is_spent_by_its_first_signature() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let key = key_manager
            .get_random_key(None, Some(LedgerKeyBranch::Random))
            .expect("a device key");
        let reserved = key_manager.reserve_ephemeral_nonce().expect("GenerateEphemeralNonce");

        let challenge = fixtures::random_challenge();
        let signature = key_manager
            .sign_with_nonce_and_challenge(&key.key_id, &reserved.key_id, &challenge)
            .expect("the first signature with a freshly reserved nonce");
        assert_signed_with(&signature, &key.pub_key, &reserved, &challenge);

        let clone = key_manager.clone();
        for (who, signer) in [("the same key manager", &key_manager), ("a clone", &clone)] {
            let error = ledger_error(
                signer
                    .sign_with_nonce_and_challenge(&key.key_id, &reserved.key_id, &fixtures::random_challenge())
                    .expect_err("a second signature over a spent nonce"),
            );
            assert!(
                error.contains("NonceHandleInvalid"),
                "{who} spending handle {} a second time should be refused as NonceHandleInvalid: {error}",
                handle(&reserved)
            );
        }

        let error = ledger_error(
            key_manager
                .sign_with_nonce_and_challenge(
                    &key.key_id,
                    &TariKeyId::LedgerEphemeralNonce {
                        handle: INVALID_NONCE_HANDLE,
                    },
                    &fixtures::random_challenge(),
                )
                .expect_err("handle zero"),
        );
        assert!(error.contains("NonceHandleInvalid"), "{error}");
    });
}

/// Nine outstanding reservations: the first is evicted, the other eight are all still good, and the refusal is
/// `NonceHandleInvalid` - never `NonceStoreFull`.
///
/// Deterministic whatever earlier tests left in the store. Handles come from a strictly increasing counter, so every
/// leftover is older than all nine of these; the first eight push the leftovers out, and the ninth pushes out the
/// oldest of this batch. The eight survivors are exactly `EPHEMERAL_NONCE_STORE_SIZE`, so this is the boundary from
/// both sides at once: a store that held seven would fail on a survivor, and one that held nine would sign with the
/// first.
#[test]
fn the_ninth_reservation_evicts_the_first_and_the_store_is_not_full() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let key = key_manager
            .get_random_key(None, Some(LedgerKeyBranch::PreMine))
            .expect("a device key");

        let reservations: Vec<TariKeyAndId> = (0..=EPHEMERAL_NONCE_STORE_SIZE)
            .map(|_| key_manager.reserve_ephemeral_nonce().expect("GenerateEphemeralNonce"))
            .collect();
        let handles: Vec<u64> = reservations.iter().map(handle).collect();
        assert!(
            handles
                .windows(2)
                .all(|pair| matches!(pair, [first, second] if first < second)),
            "the device re-issued or lowered a nonce handle: {handles:?}"
        );

        let (evicted, survivors) = reservations.split_first().expect("nine reservations");
        let error = ledger_error(
            key_manager
                .sign_with_nonce_and_challenge(&key.key_id, &evicted.key_id, &fixtures::random_challenge())
                .expect_err("the oldest of nine outstanding reservations"),
        );
        assert!(
            error.contains("NonceHandleInvalid") && !error.contains("NonceStoreFull"),
            "the {}th reservation should evict the 1st from a {EPHEMERAL_NONCE_STORE_SIZE} slot store, leaving it \
             NonceHandleInvalid - NonceStoreFull means the handle counter is exhausted, not the store: {error}",
            EPHEMERAL_NONCE_STORE_SIZE + 1
        );

        assert_eq!(survivors.len(), EPHEMERAL_NONCE_STORE_SIZE);
        for survivor in survivors {
            let challenge = fixtures::random_challenge();
            let signature = key_manager
                .sign_with_nonce_and_challenge(&key.key_id, &survivor.key_id, &challenge)
                .unwrap_or_else(|e| {
                    panic!(
                        "handle {} was one of the {EPHEMERAL_NONCE_STORE_SIZE} newest reservations and should still \
                         be in the store: {e}",
                        handle(survivor)
                    )
                });
            assert_signed_with(&signature, &key.pub_key, survivor, &challenge);
        }
    });
}

/// `get_metadata_signature` reserves exactly one device nonce for a device held sender offset key and spends it -
/// and reserves none for a host held one. Both outputs' metadata signatures verify as transaction outputs.
///
/// The first half is the change output path through `crate::test_helpers`, which is how the rest of the codebase
/// builds an output: `TestParams::new` takes its sender offset key from `get_script_offset`, and `create_output`
/// signs the metadata with it. Before the fix that path drew a host nonce for a device key, which the key manager
/// refuses, and every send with change failed.
///
/// The second half is what the fix must *not* have changed. The coinbase builder still mints a software sender
/// offset key, and a device nonce cannot sign against one, so the key manager has to keep drawing a host nonce for
/// it - asserted here as a signature that verifies and a wire that never saw `GenerateEphemeralNonce` or anything
/// else.
#[test]
fn a_metadata_signature_reserves_a_device_nonce_only_for_a_device_sender_offset_key() {
    with_device(|device| {
        let key_manager = device.key_manager();

        let params = TestParams::new(&key_manager);
        assert!(
            matches!(params.sender_offset_key_id, TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::OneSidedSenderOffset,
                ..
            }),
            "TestParams on a ledger wallet should carry a device held sender offset key, got {}",
            params.sender_offset_key_id
        );
        let (output, wire) = device.watch(|| {
            params
                .create_output(UtxoTestParams::with_value(MicroMinotari(5_000)), &key_manager)
                .expect("a change-shaped output with a device held sender offset key")
        });
        assert_eq!(
            (
                wire.count(Instruction::GenerateEphemeralNonce),
                wire.count(Instruction::GetRawSchnorrSignature)
            ),
            (1, 1),
            "signing one metadata signature should reserve exactly one device nonce and spend it: {wire:?}"
        );
        output
            .to_transaction_output()
            .expect("a transaction output")
            .verify_metadata_signature()
            .expect("the metadata signature over a device held sender offset key verifies");

        // The coinbase shape: a sender offset key the host holds.
        let host_sender_offset = key_manager.get_random_key(None, None).expect("a host key");
        let (commitment_mask, script_key) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
        let (output, wire) = device.watch(|| {
            WalletOutputBuilder::new(MicroMinotari(5_000), commitment_mask.key_id.clone())
                .with_script(script!(Nop).unwrap())
                .with_input_data(ExecutionStack::default())
                .with_script_key(script_key.key_id.clone())
                .sign_metadata_signature(&key_manager, &host_sender_offset.key_id)
                .expect("a metadata signature over a host held sender offset key")
                .try_build(&key_manager)
                .expect("a wallet output")
        });
        assert!(
            wire.is_empty(),
            "a host held sender offset key must keep a host drawn nonce and never involve the device: {wire:?}"
        );
        output
            .to_transaction_output()
            .expect("a transaction output")
            .verify_metadata_signature()
            .expect("the metadata signature over a host held sender offset key verifies");
    });
}
