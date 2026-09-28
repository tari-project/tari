// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! A request sent while the review is on screen must not change what the user approved.
//!
//! # Why this is its own test binary
//!
//! The attack is two requests on one connection with the first one still outstanding, which no well behaved
//! transport sends, so this talks to the simulator over a raw socket of its own rather than through the registered
//! transport. Speculos serves one APDU connection at a time. Under `cargo nextest` every test is its own process and
//! that is enough on its own, but under the `cargo test` fallback documented in `speculos_review.rs` the tests of
//! one binary share a process - and a registered transport left connected by an earlier test in it would be talking
//! to a simulator that had moved on to this test's socket. A binary of its own keeps both runners honest.
//!
//! # Running it
//!
//! Exactly as `speculos_review.rs` describes, with `--test speculos_review_during` in place of
//! `--test speculos_review`. `scripts/ledger_speculos.sh test` runs it with everything else.

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

use minotari_ledger_wallet_common::{
    codec::{ComAndPubSigReply, Decode, GetOneSidedMetadataSignatureRequest, GetPublicKeyRequest, KeyReply},
    common_types::LedgerKeyBranch,
};
use minotari_ledger_wallet_comms::ledger_wallet::Command;
use minotari_ledger_wallet_comms_testing::{
    approver::{Approver, SpeculosApprover},
    fixtures,
    review::ExpectedReview,
    simulator,
    speculos_api::joined,
};
use tari_common_types::{tari_address::TariAddress, types::CompressedPublicKey};
use tari_crypto::{
    commitment::HomomorphicCommitment,
    keys::{PublicKey, SecretKey},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    signatures::CommitmentAndPublicKeySignature,
};
use tari_utilities::ByteArray;

/// The account to derive the sender offset key from. Any account will do; a constant keeps runs comparable.
const ACCOUNT: u64 = 0;
/// The sender offset key index. Same reasoning.
const SENDER_OFFSET_KEY_INDEX: u64 = 7;
/// Under a million, so the device renders it in microTari, as `review::minotari_amount` expects.
const VALUE: u64 = 12_345;
const STATUS_OK: u16 = 0x9000;

/// One framed APDU on the Speculos socket: a 4-byte big endian length, then the APDU.
fn send_raw(stream: &mut TcpStream, command: &Command<Vec<u8>>) {
    let apdu = command.to_apdu_command().serialize();
    let mut framed = u32::try_from(apdu.len())
        .expect("an APDU fits a u32")
        .to_be_bytes()
        .to_vec();
    framed.extend_from_slice(&apdu);
    stream.write_all(&framed).expect("write the APDU to Speculos");
    stream.flush().expect("flush the APDU to Speculos");
}

/// One reply off the Speculos socket: `(data, status word)`, or `None` if nothing arrives before the stream's read
/// timeout.
fn read_raw(stream: &mut TcpStream) -> Option<(Vec<u8>, u16)> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).ok()?;
    let length = usize::try_from(u32::from_be_bytes(length)).expect("a reply length fits a usize");
    let mut reply = vec![0u8; length + 2];
    stream
        .read_exact(&mut reply)
        .expect("a reply that has started must finish");
    let status = u16::from_be_bytes([reply[length], reply[length + 1]]);
    reply.truncate(length);
    Some((reply, status))
}

fn exchange_raw(stream: &mut TcpStream, command: &Command<Vec<u8>>) -> (Vec<u8>, u16) {
    send_raw(stream, command);
    read_raw(stream).expect("the device must answer")
}

/// Wait until something other than the home screen is up, i.e. the review has been drawn.
fn wait_for_review(approver: &SpeculosApprover) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let screen = joined(&approver.api().current_screen().expect("read the screen"));
        if !screen.is_empty() && !screen.contains("MinoTari") {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the review never appeared");
}

/// The request the attack sends twice, once per receiver.
fn metadata_request(mask: &[u8; 32], address: &[u8], message: &[u8; 32], network: u8) -> Command<Vec<u8>> {
    Command::from_request(
        &GetOneSidedMetadataSignatureRequest::new(
            ACCOUNT,
            u64::from(network),
            0,
            SENDER_OFFSET_KEY_INDEX,
            VALUE,
            mask,
            address,
            message,
        )
        .expect("a dual address fits its length prefix"),
    )
}

/// Whether `reply` is a metadata signature over the stealth script for `spend_key`.
fn signs_for(
    reply: &[u8],
    sender_offset_public_key: &RistrettoPublicKey,
    mask: &RistrettoSecretKey,
    spend_key: &RistrettoPublicKey,
    message: &[u8; 32],
    network: u8,
) -> bool {
    let reply = ComAndPubSigReply::decode(reply).expect("a 161 byte signature reply");
    let key = |bytes: &[u8; 32]| RistrettoPublicKey::from_canonical_bytes(bytes).expect("a valid point");
    let scalar = |bytes: &[u8; 32]| RistrettoSecretKey::from_canonical_bytes(bytes).expect("a canonical scalar");
    let ephemeral_commitment = HomomorphicCommitment::from_public_key(&key(reply.ephemeral_commitment));
    let ephemeral_pubkey = key(reply.ephemeral_pubkey);

    let commitment = fixtures::script_commitment(mask, &RistrettoSecretKey::from(VALUE));
    let script = fixtures::stealth_script(mask, spend_key);
    let message = fixtures::metadata_signature_message(network, &script, message);
    let challenge = fixtures::metadata_signature_challenge(
        network,
        sender_offset_public_key,
        &ephemeral_commitment,
        &ephemeral_pubkey,
        &commitment,
        &message,
    );
    CommitmentAndPublicKeySignature::new(
        ephemeral_commitment,
        ephemeral_pubkey,
        scalar(reply.u_a),
        scalar(reply.u_x),
        scalar(reply.u_y),
    )
    .verify_challenge(
        &commitment,
        sender_offset_public_key,
        &challenge,
        &fixtures::commitment_factory(),
        &mut rand::rng(),
    )
}

/// Security: a request sent while the review is on screen cannot change what the user approved.
///
/// The device reads a request out of the SDK's APDU buffer, and on Stax and Flex that buffer is rewritten by any
/// APDU that arrives while an NBGL screen is polling for events (`ledger_device_sdk` 1.35.0, `io_legacy.rs`,
/// `decode_event`). A handler that re-read the receiver address after the review would then build its script from
/// whatever the host sent last - so a host could show the user receiver A, send receiver B mid-review, and collect
/// a signature paying B for a review of A. The handler now copies everything it uses after the review before it;
/// this is the end-to-end check that it did.
///
/// Against the firmware before the fix, on `stax`, this fails with the device having signed for B.
///
/// It runs on every model. The buffer overwrite is an NBGL behaviour, but the property - the device signs what it
/// showed - is not, and asserting it everywhere costs nothing.
#[test]
#[ignore = "needs a running Speculos simulator; see the module docs"]
fn a_request_sent_during_the_review_cannot_change_the_approved_receiver() {
    let approver = SpeculosApprover::from_env().expect("SPECULOS_API_ADDRESS / SPECULOS_MODEL");
    approver.expect_home().expect("the device should start at home");

    let mut stream = TcpStream::connect(simulator::apdu_address().expect("SPECULOS_APDU_ADDRESS"))
        .expect("connect to the Speculos APDU socket");

    // Receiver A is the one the user is shown. Receiver B is the attacker's: the same view key and network, a spend
    // key the host controls, and a valid checksum, so it passes every check the device makes on an address.
    let approved = fixtures::published_receiver(0).expect("the published address must parse");
    let attacker_spend_key = RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut rand::rng()));
    let attacker = TariAddress::new_dual_address(
        approved.public_view_key().expect("a dual address").clone(),
        CompressedPublicKey::new_from_pk(attacker_spend_key.clone()),
        approved.network(),
        approved.features(),
        None,
    )
    .expect("a valid attacker address");
    assert_eq!(attacker.to_vec().len(), approved.to_vec().len());

    let network = approved.network().as_byte();
    let mask = RistrettoSecretKey::from(42u64);
    let mask_bytes: [u8; 32] = mask.as_bytes().try_into().expect("32 bytes");
    let message = [7u8; 32];

    // The sender offset key the signature is made with, asked for over the same socket.
    let (reply, status) = exchange_raw(
        &mut stream,
        &Command::from_request(&GetPublicKeyRequest {
            account: ACCOUNT,
            index: SENDER_OFFSET_KEY_INDEX,
            branch: u64::from(LedgerKeyBranch::OneSidedSenderOffset.as_byte()),
        }),
    );
    assert_eq!(status, STATUS_OK, "GetPublicKey");
    let sender_offset_public_key =
        RistrettoPublicKey::from_canonical_bytes(KeyReply::decode(&reply).expect("a key reply").key)
            .expect("a valid point");

    // Request A, and wait for its review to be drawn.
    send_raw(
        &mut stream,
        &metadata_request(&mask_bytes, &approved.to_vec(), &message, network),
    );
    wait_for_review(&approver);

    // Request B, while A's review is on screen and A's request is still outstanding.
    send_raw(
        &mut stream,
        &metadata_request(&mask_bytes, &attacker.to_vec(), &message, network),
    );
    std::thread::sleep(Duration::from_millis(500));

    // The user reviews A - the screen was drawn before B arrived, so it does show A - and approves it.
    let expected = ExpectedReview::one_sided_metadata_signature(VALUE, &approved.to_base58(), 0);
    approver
        .expect_and_approve(&expected)
        .unwrap_or_else(|e| panic!("the review of A: {e}"));

    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("set a read timeout");
    let (signature, status) = read_raw(&mut stream).expect("the approved request must be answered");
    assert_eq!(status, STATUS_OK, "the approved request must succeed");

    let approved_spend_key = approved.public_spend_key().to_public_key().expect("a valid spend key");
    let signs_approved = signs_for(
        &signature,
        &sender_offset_public_key,
        &mask,
        &approved_spend_key,
        &message,
        network,
    );
    let signs_attacker = signs_for(
        &signature,
        &sender_offset_public_key,
        &mask,
        &attacker_spend_key,
        &message,
        network,
    );
    assert!(
        !signs_attacker,
        "the device signed a script paying the attacker's spend key, although the user approved the original \
         receiver: a request sent during the review changed what was signed"
    );
    assert!(
        signs_approved,
        "the signature must be over the receiver the user approved"
    );

    // Whatever the device did with request B, leave it at home: if B was served as a request of its own, its
    // review is rejected here (a mismatch rejects too) and its answer drained.
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set a read timeout");
    if approver.wait_for_home().is_err() {
        let expected_b = ExpectedReview::one_sided_metadata_signature(VALUE, &attacker.to_base58(), 0);
        let _ = approver.expect_and_reject(&expected_b);
    }
    let late = read_raw(&mut stream);
    println!("after the approval, request B was answered with {late:?}");
    approver.wait_for_home().expect("the device should be back at home");
}
