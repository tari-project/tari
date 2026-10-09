// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! ④ `ledger_get_one_sided_metadata_signature_wrapper`, through
//! `WalletOutputBuilder::sign_metadata_signature_user_verified`.
//!
//! It is the only way a `OneSidedSenderOffset` key signs anything. (`PreMine` sender offset keys, which a host can
//! mint on demand through `get_script_offset` with a `PreMine` script key, still sign raw challenges; that belongs
//! with the separate pre-mine issue.) Change is signed through it too, with the wallet's own address. The device is
//! sent the output's raw common fields and hashes them itself, so it signs an output to the wallet's own address with
//! default features without a review, and reviews anything else - showing what is not default.
//!
//! `GetOneSidedMetadataSignature` is the only instruction in the application that puts anything in front of a
//! human: it shows the amount, the receiver and any payment ID, and does not answer until somebody approves or
//! rejects. So this is the one group that needs the [`Approver`](minotari_ledger_wallet_comms_testing::approver::
//! Approver), and it uses `comms_testing`'s own - the same `SpeculosApprover`, `ExpectedReview` and
//! `while_reviewing` that its scenario library uses - rather than a second way of reading a screen.
//!
//! Each test asserts two independent things:
//!
//! 1. **What the device showed.** `ExpectedReview` compares the review for *equality* against `Amount`, `Receiver` and
//!    `Payment ID` - present with its length when the address carries one, and asserted absent when it does not - so an
//!    address with extra characters on the end, or a payment ID row nobody asked for, fails.
//! 2. **What the device signed.** The output is built the way a one-sided send builds it - a stealth `PushPubKey`
//!    script from `stealth_address_script_spending_key`, a sender offset key from `get_script_offset` - and then
//!    verified as a `TransactionOutput`. The device never sees that script: it rebuilds its own from the receiver it
//!    displayed, so a signature that verifies against the host's script is the statement that what the user approved
//!    and what the device signed are the same transaction.
//!
//! The approver blocks on the device drawing each screen; there is no sleep and no retry anywhere in it, and this
//! puts its timing on the merge path on purpose.

use minotari_ledger_wallet_common::common_types::{Instruction, LedgerKeyBranch};
use minotari_ledger_wallet_comms::accessor_methods::ledger_get_one_sided_metadata_signature;
use minotari_ledger_wallet_comms_testing::{
    approver::{Outcome, while_reviewing},
    fixtures,
    review::{ExpectedReview, MATURITY_FIELD},
};
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    types::{CompressedPublicKey, PrivateKey},
};
use tari_crypto::keys::SecretKey;
use tari_script::{ExecutionStack, push_pubkey_script, script};
use tari_transaction_components::{
    MicroMinotari,
    crypto_factories::CryptoFactories,
    key_manager::{
        KeyManager,
        SecretTransactionKeyManagerInterface,
        TariKeyAndId,
        TariKeyId,
        TransactionKeyManagerInterface,
        error::KeyManagerError,
    },
    test_helpers::{TestParams, UtxoTestParams, create_consensus_constants, create_consensus_manager},
    transaction_builder::{PendingOutput, RecipientSpec, TransactionBuilder, TransactionBuilderError},
    transaction_components::{
        MemoField,
        OutputFeatures,
        Transaction,
        TransactionError,
        TransactionOutput,
        WalletOutput,
        WalletOutputBuilder,
    },
    validation::transaction::TransactionInternalConsistencyValidator,
};

use crate::harness::{Device, network, with_device};

/// Build a one-sided output of `value` to the published receiver carrying `payment_id_length` payment ID bytes, sign
/// its metadata on the device, and answer the review with `outcome` - asserting what it showed first.
fn one_sided_output(
    device: &Device,
    value: u64,
    payment_id_length: usize,
    outcome: Outcome,
) -> Result<WalletOutput, TransactionError> {
    one_sided_output_from(device, value, payment_id_length, outcome, false)
}

/// [`one_sided_output`], with `pre_mine` choosing the script key: an alpha derived one (an ordinary send, so the
/// sender offset key is on `OneSidedSenderOffset`), or a `PreMine` one (the backup pre-mine spend, so
/// `get_script_offset` is in pre-mine mode and the sender offset key is on `PreMine`).
fn one_sided_output_from(
    device: &Device,
    value: u64,
    payment_id_length: usize,
    outcome: Outcome,
    pre_mine: bool,
) -> Result<WalletOutput, TransactionError> {
    let key_manager = device.key_manager();
    let (commitment_mask, derived_script_key) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
    let script_key = if pre_mine {
        key_manager
            .get_random_key(None, Some(LedgerKeyBranch::PreMine))
            .expect("a pre-mine script key")
    } else {
        derived_script_key
    };
    let (_offset, mut sender_offsets) = key_manager
        .get_script_offset(std::slice::from_ref(&script_key.key_id), 1)
        .expect("a sender offset key from the device");
    let sender_offset = sender_offsets.pop().expect("one sender offset key");
    let expected_branch = if pre_mine {
        LedgerKeyBranch::PreMine
    } else {
        LedgerKeyBranch::OneSidedSenderOffset
    };
    assert!(
        matches!(sender_offset.key_id, TariKeyId::LedgerKey { branch, .. } if branch == expected_branch),
        "expected a {expected_branch} sender offset key, got {}",
        sender_offset.key_id
    );

    let receiver = fixtures::published_receiver(payment_id_length).unwrap_or_else(|e| panic!("{e}"));
    let stealth_key = key_manager
        .stealth_address_script_spending_key(&commitment_mask.key_id, receiver.public_spend_key())
        .expect("the receiver's stealth script key");

    let builder = WalletOutputBuilder::new(MicroMinotari(value), commitment_mask.key_id.clone())
        .with_script(push_pubkey_script(&stealth_key))
        .with_input_data(ExecutionStack::default())
        .encrypt_data_for_recovery(&key_manager, None, MemoField::default())
        .expect("encrypted data")
        .with_script_key(script_key.key_id.clone());

    let expected = ExpectedReview::one_sided_metadata_signature(value, &receiver.to_base58(), payment_id_length);
    let (signed, review) = while_reviewing(device.approver(), &expected, outcome, || {
        builder.sign_metadata_signature_user_verified(&key_manager, &sender_offset.key_id, &receiver)
    });
    review.unwrap_or_else(|e| panic!("the device's review of {}: {e}", expected.summary()));

    let output = signed?.try_build(&key_manager)?;
    assert_eq!(
        output.sender_offset_public_key(),
        &sender_offset.pub_key,
        "the output carries a different sender offset key from the one the device signed with"
    );
    Ok(output)
}

fn assert_metadata_signature_verifies(output: &WalletOutput) {
    output
        .to_transaction_output()
        .expect("a transaction output")
        .verify_metadata_signature()
        .expect("the metadata signature the device produced after approval verifies as a transaction output's");
}

/// The review shows `Amount`, `Receiver` and `Payment ID`, and once approved the signature verifies.
///
/// Eight payment ID bytes, so the device has all three rows to draw, and the `Payment ID` row's value - its length -
/// is compared like the others.
#[test]
fn the_review_shows_amount_receiver_and_payment_id_and_the_approved_signature_verifies() {
    with_device(|device| {
        let output = one_sided_output(device, 12_345, 8, Outcome::Approve).expect("an approved one-sided output");
        assert_metadata_signature_verifies(&output);
    });
}

/// The backup pre-mine spend: the only script key is a `PreMine` key, so the sender offset key is on `PreMine` too,
/// and the device has to derive it there - the review is the same, and the approved signature verifies against the
/// `PreMine` sender offset key. Before the request carried the branch, the device derived on `OneSidedSenderOffset`
/// regardless and the output was invalid.
#[test]
fn a_pre_mine_sender_offset_key_is_reviewed_and_signs_on_its_own_branch() {
    with_device(|device| {
        let output =
            one_sided_output_from(device, 12_345, 0, Outcome::Approve, true).expect("an approved pre-mine output");
        assert_metadata_signature_verifies(&output);
    });
}

/// With no payment ID in the address there is no `Payment ID` row at all, and the review is asserted to lack one.
#[test]
fn a_receiver_without_a_payment_id_shows_no_payment_id_row() {
    with_device(|device| {
        let output = one_sided_output(device, 12_345, 0, Outcome::Approve).expect("an approved one-sided output");
        assert_metadata_signature_verifies(&output);
    });
}

/// An amount of a million microTari or more is shown in Tari, to two decimal places.
///
/// The Spec 4 approval scenario deliberately stays under a million, so the device's `{:.2} T` branch had never been
/// compared against the host's copy of the formatter (`review::minotari_amount`). It is here, and not in the scenario
/// library, because that library holds to exactly one approval scenario so that a hardware run costs one button
/// press; this suite runs unattended and pays nothing for a second screen.
#[test]
fn an_amount_of_a_million_micro_tari_or_more_is_shown_in_tari() {
    with_device(|device| {
        let output = one_sided_output(device, 12_345_678, 0, Outcome::Approve).expect("an approved one-sided output");
        assert_metadata_signature_verifies(&output);
    });
}

/// A rejected review is an error from the key manager, not a signature and not a hang, and the device goes home.
///
/// The review is still asserted before it is rejected - the approver never answers a screen it has not checked -
/// and the rejection has to reach the caller as the user's cancellation rather than as some malformed-reply error:
/// a wallet has to be able to tell "the user said no" from "the device is broken".
#[test]
fn a_rejected_review_signs_nothing_and_says_the_user_cancelled() {
    with_device(|device| {
        let error = one_sided_output(device, 12_345, 0, Outcome::Reject)
            .map(|output| output.sender_offset_public_key().clone())
            .expect_err("a rejected review must not produce a signed output");
        // `LedgerDeviceError::UserCancelled`'s own text, carried up through the key manager and the builder.
        let cancelled = "User cancelled the transaction";
        assert!(
            error.to_string().contains(cancelled),
            "a rejected review should surface as the user's cancellation ('{cancelled}'), got: {error}"
        );
    });
}

/// The wallet's own address, as `TransactionBuilder::new` builds it for change.
fn own_address(key_manager: &KeyManager) -> TariAddress {
    TariAddress::new_dual_address(
        key_manager.get_view_key().pub_key,
        key_manager.get_spend_key().pub_key,
        network(),
        TariAddressFeatures::create_one_sided_only(),
        None,
    )
    .expect("the wallet's own address")
}

/// A device held sender offset key, issued by `get_script_offset` for an alpha derived script key.
fn device_sender_offset_key(key_manager: &KeyManager) -> TariKeyAndId {
    let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
    let (_offset, mut sender_offsets) = key_manager
        .get_script_offset(std::slice::from_ref(&script_key.key_id), 1)
        .expect("a sender offset key from the device");
    sender_offsets.pop().expect("one sender offset key")
}

/// A change-shaped output to the wallet's own address with default features is signed with no review; the same
/// output to a foreign receiver is reviewed. Both verify, and neither involves a raw Schnorr signature or a nonce
/// reservation. Nothing answers a review for the first, so a device that showed one would leave the exchange
/// outstanding until the read timeout and fail this test.
///
/// The output is built the way `TransactionBuilder::build_change` builds one: the script is `PushPubKey` of the
/// alpha derived script key generated with the commitment mask, which is the stealth script to the wallet's own
/// address.
#[test]
fn an_own_address_output_with_default_features_is_silent_and_a_foreign_one_is_reviewed() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let own_address = own_address(&key_manager);
        let value = 12_345;

        let (commitment_mask, change_script_key) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
        let sender_offset = device_sender_offset_key(&key_manager);
        let builder = WalletOutputBuilder::new(MicroMinotari(value), commitment_mask.key_id.clone())
            .with_script(push_pubkey_script(&change_script_key.pub_key))
            .with_input_data(ExecutionStack::default())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::default())
            .expect("encrypted data")
            .with_script_key(change_script_key.key_id.clone());

        let (signed, wire) = device.watch(|| {
            builder
                .clone()
                .sign_metadata_signature_user_verified(&key_manager, &sender_offset.key_id, &own_address)
        });
        assert_eq!(
            (
                wire.count(Instruction::GetOneSidedMetadataSignature),
                wire.count(Instruction::GetRawSchnorrSignature),
                wire.count(Instruction::GenerateEphemeralNonce),
            ),
            // The head and one preimage chunk.
            (2, 0, 0),
            "the output should be signed by the metadata signature instruction alone: {wire:?}"
        );
        let output = signed
            .expect("an output to the device's own address, signed with no review")
            .try_build(&key_manager)
            .expect("a wallet output");
        assert_metadata_signature_verifies(&output);

        // The same value and commitment mask, to a foreign receiver: the stealth script to that receiver, reviewed.
        let receiver = fixtures::published_receiver(0).unwrap_or_else(|e| panic!("{e}"));
        let stealth_key = key_manager
            .stealth_address_script_spending_key(&commitment_mask.key_id, receiver.public_spend_key())
            .expect("the receiver's stealth script key");
        let sender_offset = device_sender_offset_key(&key_manager);
        let builder = builder.with_script(push_pubkey_script(&stealth_key));
        let expected = ExpectedReview::one_sided_metadata_signature(value, &receiver.to_base58(), 0);
        let (signed, review) = while_reviewing(device.approver(), &expected, Outcome::Approve, || {
            builder.sign_metadata_signature_user_verified(&key_manager, &sender_offset.key_id, &receiver)
        });
        review.unwrap_or_else(|e| panic!("the device's review of {}: {e}", expected.summary()));
        let output = signed
            .expect("an approved output to a foreign receiver")
            .try_build(&key_manager)
            .expect("a wallet output");
        assert_metadata_signature_verifies(&output);
    });
}

/// An output to the wallet's own address with a maturity is reviewed, and the review shows the maturity: before the
/// device read the features, it would have signed such a "change" - a freeze of the funds - silently.
#[test]
fn an_own_address_output_with_a_maturity_is_reviewed_showing_it() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let own_address = own_address(&key_manager);
        let value = 12_345;
        let maturity = 1_000_000;

        let (commitment_mask, change_script_key) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
        let sender_offset = device_sender_offset_key(&key_manager);
        let builder = WalletOutputBuilder::new(MicroMinotari(value), commitment_mask.key_id.clone())
            .with_features(OutputFeatures {
                maturity,
                ..Default::default()
            })
            .with_script(push_pubkey_script(&change_script_key.pub_key))
            .with_input_data(ExecutionStack::default())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::default())
            .expect("encrypted data")
            .with_script_key(change_script_key.key_id.clone());

        let expected = ExpectedReview::one_sided_metadata_signature(value, &own_address.to_base58(), 0)
            .with_output_field(MATURITY_FIELD, &maturity.to_string());
        let (signed, review) = while_reviewing(device.approver(), &expected, Outcome::Approve, || {
            builder.sign_metadata_signature_user_verified(&key_manager, &sender_offset.key_id, &own_address)
        });
        review.unwrap_or_else(|e| panic!("the device's review of {}: {e}", expected.summary()));
        let output = signed
            .expect("an approved output with a maturity")
            .try_build(&key_manager)
            .expect("a wallet output");
        assert_metadata_signature_verifies(&output);
    });
}

/// A host that lies to the device about an output's features gets a signature that verifies on no output carrying
/// the features it publishes.
///
/// The host tells the device the output has default features - which the device signs silently for the wallet's
/// own address - and publishes it with a maturity instead. The signature verifies on the output the device was told
/// about, and not on the one the host published, because the device hashed the fields it read and consensus hashes
/// the published output's own.
#[test]
fn a_host_that_lies_about_the_features_gets_a_signature_that_does_not_verify() {
    with_device(|device| {
        let account = fixtures::random_u64();
        let key_manager = device.key_manager_for(account);
        let own_address = own_address(&key_manager);
        let value = 12_345;

        let (commitment_mask, change_script_key) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
        let sender_offset = device_sender_offset_key(&key_manager);
        let TariKeyId::LedgerKey { branch, index } = sender_offset.key_id else {
            panic!("a device held sender offset key");
        };
        // One builder, so the two outputs differ in their features and nothing else - the encrypted data included.
        let base = WalletOutputBuilder::new(MicroMinotari(value), commitment_mask.key_id.clone())
            .with_script(push_pubkey_script(&change_script_key.pub_key))
            .with_input_data(ExecutionStack::default())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::default())
            .expect("encrypted data")
            .with_script_key(change_script_key.key_id.clone())
            .with_sender_offset_public_key(sender_offset.pub_key.clone());
        let shape = |features: OutputFeatures| {
            base.clone()
                .with_features(features)
                .with_place_holder_metadata_signature(&key_manager, &sender_offset.key_id)
                .expect("a placeholder signature")
                .try_build(&key_manager)
                .expect("a wallet output")
                .to_transaction_output()
                .expect("a transaction output")
        };
        let told = shape(OutputFeatures::default());
        let published = shape(OutputFeatures {
            maturity: 1_000_000,
            ..Default::default()
        });

        let preimage = TransactionOutput::metadata_signature_message_common_preimage(
            &told.version,
            &told.features,
            &told.covenant,
            &told.encrypted_data,
            &told.minimum_value_promise,
        )
        .expect("a preimage");
        let signature = ledger_get_one_sided_metadata_signature(
            account,
            network(),
            value,
            index,
            branch,
            &key_manager
                .get_private_key(&commitment_mask.key_id)
                .expect("the commitment mask"),
            &own_address,
            &preimage,
        )
        .expect("default features to the device's own address, signed with no review");

        let mut told = told;
        told.metadata_signature = signature.clone();
        told.verify_metadata_signature()
            .expect("the signature verifies on the output the device was told about");
        let mut published = published;
        published.metadata_signature = signature;
        assert!(
            published.verify_metadata_signature().is_err(),
            "the signature must not verify on an output with features the device was not shown"
        );
    });
}

/// An output whose script is not the stealth script to its address - a burn's `Nop`, here - is refused by a ledger
/// wallet before the device is asked: the device signs over the stealth script it builds itself, so any other
/// script would get a signature that does not verify.
#[test]
fn an_output_with_a_custom_script_is_refused_before_the_device() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let own_address = own_address(&key_manager);
        let commitment_mask = key_manager.get_next_commitment_mask_and_script_key().unwrap().0;
        let sender_offset = device_sender_offset_key(&key_manager);
        let builder = WalletOutputBuilder::new(MicroMinotari(12_345), commitment_mask.key_id.clone())
            .with_script(script!(Nop).unwrap())
            .with_input_data(ExecutionStack::default())
            .with_script_key(TariKeyId::Zero);

        let (error, wire) = device.watch(|| {
            builder
                .sign_metadata_signature_user_verified(&key_manager, &sender_offset.key_id, &own_address)
                .map(|_| ())
                .expect_err("a custom script on a ledger wallet")
        });
        assert!(
            error
                .to_string()
                .contains(&KeyManagerError::LedgerSenderOffsetNeedsRecipient.to_string()),
            "expected the key manager's refusal, got: {error}"
        );
        assert_eq!(
            wire.count(Instruction::GetOneSidedMetadataSignature),
            0,
            "the refusal must come before the device is asked to sign: {wire:?}"
        );
    });
}

/// An output this ledger wallet owns and can spend: its script key is the alpha derived key the device signs for.
///
/// Built through `TestParams`, whose metadata signature is made with no recipient address. A ledger wallet refuses
/// that for a device held sender offset key, and the output being spent is only the fixture here, so it carries a
/// host held one.
fn ledger_input(key_manager: &KeyManager, value: u64) -> WalletOutput {
    let mut params = TestParams::new(key_manager);
    let host_sender_offset = key_manager.get_random_key(None, None).expect("a host key");
    params.sender_offset_key_id = host_sender_offset.key_id;
    params.sender_offset_key_pk = host_sender_offset.pub_key;
    params.create_input(UtxoTestParams::with_value(MicroMinotari(value)), key_manager)
}

/// A transaction builder on `key_manager`, spending one input of `value`.
fn ledger_builder(key_manager: &KeyManager, value: u64) -> TransactionBuilder<KeyManager> {
    let input = ledger_input(key_manager, value);
    let mut builder = TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), network())
        .expect("a transaction builder");
    builder
        .with_fee_per_gram(MicroMinotari(5))
        .with_input(input)
        .expect("the input");
    builder
}

/// The transaction is internally consistent: every metadata and script signature verifies, and the script offset is
/// the input script keys minus the output sender offset keys.
fn assert_transaction_validates(transaction: &Transaction) {
    TransactionInternalConsistencyValidator::new(false, create_consensus_manager(), CryptoFactories::default())
        .validate(transaction, None, None, u64::MAX)
        .expect("the transaction the ledger wallet built validates");
}

/// A real send with change, through `TransactionBuilder`: the recipient's output is reviewed, and the change output
/// - signed in `build_change` and re-signed when the final fee is written into its memo - is signed with no review.
///
/// The approver answers exactly one review, the recipient's. A device that also put the change up for review would
/// leave that exchange outstanding until the read timeout and fail this test. The wire shows no raw Schnorr
/// signature and no nonce reservation, and the finalised transaction validates.
#[test]
fn a_send_with_change_reviews_the_recipient_and_signs_the_change_without_a_review() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let receiver = fixtures::published_receiver(0).unwrap_or_else(|e| panic!("{e}"));
        let value = 12_345;

        let mut builder = ledger_builder(&key_manager, 1_000_000);
        builder
            .add_stealth_recipient(
                receiver.clone(),
                MicroMinotari(value),
                OutputFeatures::default(),
                MemoField::new_empty(),
            )
            .expect("the recipient");
        builder.reserve_sender_offset_keys(&[]).expect("the sender offset keys");

        let expected = ExpectedReview::one_sided_metadata_signature(value, &receiver.to_base58(), 0);
        let ((finalized, review), wire) =
            device.watch(|| while_reviewing(device.approver(), &expected, Outcome::Approve, || builder.build()));
        review.unwrap_or_else(|e| panic!("the device's review of {}: {e}", expected.summary()));
        let finalized = finalized.expect("a finalised send with change");

        assert!(finalized.change.is_some(), "the send should have produced change");
        assert_eq!(
            (
                wire.count(Instruction::GetRawSchnorrSignature),
                wire.count(Instruction::GenerateEphemeralNonce),
            ),
            (0, 0),
            "no output may be signed through the raw instruction: {wire:?}"
        );
        assert!(
            wire.count(Instruction::GetOneSidedMetadataSignature) >= 4,
            "the recipient and the change should both be signed by the metadata signature instruction: {wire:?}"
        );
        assert_transaction_validates(&finalized.transaction);
    });
}

/// A `RecipientSpec::to_self` output, through `TransactionBuilder`: it and the change are both outputs to this wallet
/// with default features, so the device signs the whole transaction with no review at all. Nothing answers a review
/// here. The wire shows no raw Schnorr signature and no nonce reservation, and the finalised transaction validates.
#[test]
fn a_to_self_output_and_its_change_are_signed_without_a_review() {
    with_device(|device| {
        let key_manager = device.key_manager();

        let mut builder = ledger_builder(&key_manager, 1_000_000);
        builder
            .with_recipient_spec(RecipientSpec::to_self(
                MicroMinotari(12_345),
                OutputFeatures::default(),
                MemoField::new_empty(),
            ))
            .expect("the to-self output");
        builder.reserve_sender_offset_keys(&[]).expect("the sender offset keys");

        let (finalized, wire) = device.watch(|| builder.build());
        let finalized = finalized.expect("a finalised transaction to self");

        assert!(
            finalized.change.is_some(),
            "the transaction should have produced change"
        );
        assert_eq!(
            (
                wire.count(Instruction::GetRawSchnorrSignature),
                wire.count(Instruction::GenerateEphemeralNonce),
            ),
            (0, 0),
            "no output may be signed through the raw instruction: {wire:?}"
        );
        assert!(
            wire.count(Instruction::GetOneSidedMetadataSignature) >= 4,
            "the to-self output and the change should both be signed by the metadata signature instruction: {wire:?}"
        );
        assert_transaction_validates(&finalized.transaction);
    });
}

/// An offline signing payload's pre-built (custom) output, on a ledger wallet, is refused when it is added - before
/// `build`, and so before the device is asked to review the recipient. This is the sequence the offline one-sided
/// signer runs: a recipient, one reservation covering the custom output too, then the custom output with the
/// device held sender offset key the reservation issued for it. Without the early refusal the user would approve the
/// recipient's review and the build would then fail on the custom output's re-sign.
#[test]
fn a_custom_output_with_a_device_sender_offset_is_refused_before_any_review() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let receiver = fixtures::published_receiver(0).unwrap_or_else(|e| panic!("{e}"));
        let custom_output = ledger_input(&key_manager, 5_000);

        let (error, wire) = device.watch(|| {
            let mut builder = ledger_builder(&key_manager, 1_000_000);
            builder
                .add_stealth_recipient(
                    receiver.clone(),
                    MicroMinotari(12_345),
                    OutputFeatures::default(),
                    MemoField::new_empty(),
                )
                .expect("the recipient");
            let pending = PendingOutput::from_output(&custom_output).expect("a pending custom output");
            let mut keys = builder
                .reserve_sender_offset_keys(&[pending])
                .expect("the sender offset keys");
            let sender_offset = keys.pop().expect("the custom output's sender offset key");
            assert!(
                matches!(sender_offset.key_id, TariKeyId::LedgerKey { .. }),
                "expected a device held sender offset key, got {}",
                sender_offset.key_id
            );
            builder
                .with_output(custom_output.clone(), sender_offset.key_id, None)
                .map(|_| ())
                .expect_err("a custom output with a device held sender offset key on a ledger wallet")
        });
        assert!(
            matches!(
                error,
                TransactionBuilderError::KeyManagerError(KeyManagerError::LedgerSenderOffsetNeedsRecipient)
            ),
            "got {error:?}"
        );
        assert_eq!(
            wire.count(Instruction::GetOneSidedMetadataSignature),
            0,
            "nothing may be put up for review before the refusal: {wire:?}"
        );
    });
}

/// An address carrying this wallet's own spend key but another view key is not change: the device shows the full
/// review, and the approved signature verifies. The host derives the output's commitment mask and encrypted data from
/// the view key, so such an output would be locked to this wallet yet invisible to its scanner.
#[test]
fn own_spend_key_with_a_foreign_view_key_is_reviewed() {
    with_device(|device| {
        let key_manager = device.key_manager();
        let receiver = TariAddress::new_dual_address(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
            key_manager.get_spend_key().pub_key,
            network(),
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .expect("an address with this wallet's spend key and a foreign view key");
        let value = 12_345;

        let (commitment_mask, _) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
        let stealth_key = key_manager
            .stealth_address_script_spending_key(&commitment_mask.key_id, receiver.public_spend_key())
            .expect("the stealth script key");
        let sender_offset = device_sender_offset_key(&key_manager);
        let builder = WalletOutputBuilder::new(MicroMinotari(value), commitment_mask.key_id.clone())
            .with_script(push_pubkey_script(&stealth_key))
            .with_input_data(ExecutionStack::default())
            .encrypt_data_for_recovery(&key_manager, None, MemoField::default())
            .expect("encrypted data")
            .with_script_key(TariKeyId::Zero);

        let expected = ExpectedReview::one_sided_metadata_signature(value, &receiver.to_base58(), 0);
        let (signed, review) = while_reviewing(device.approver(), &expected, Outcome::Approve, || {
            builder.sign_metadata_signature_user_verified(&key_manager, &sender_offset.key_id, &receiver)
        });
        review.unwrap_or_else(|e| panic!("the device's review of {}: {e}", expected.summary()));
        let output = signed
            .expect("an approved output to this wallet's spend key with a foreign view key")
            .try_build(&key_manager)
            .expect("a wallet output");
        assert_metadata_signature_verifies(&output);
    });
}
