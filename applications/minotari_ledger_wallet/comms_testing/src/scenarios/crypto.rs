// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! What the device returned, checked as mathematics rather than against frozen bytes.
//!
//! # Why not golden signatures
//!
//! A frozen signature is a strictly weaker statement than the one made here, and a more fragile one. Every
//! signature the device produces is over a nonce it drew itself, so freezing one would pin the nonce - the single
//! value that *must* be different every time - and any device that got the nonce right would fail. What can be
//! frozen is a derived key, and that is [`super::vectors`]' job.
//!
//! So every assertion in this module is an equation:
//!
//! * a signature verifies against the public key the device also returns;
//! * a script offset equals the sum of the script keys minus the sum of the sender offset keys;
//! * a Diffie-Hellman shared secret equals the host's own scalar times the device's public key.
//!
//! Each holds for **any** input, which is why the inputs are drawn fresh on every run (see [`crate::fixtures`]).
//!
//! # How a public key statement is made about a secret the host never sees
//!
//! Every relation here is checked in the group rather than in the scalar field, which is what makes it checkable at
//! all. The device will not part with a script key, a sender offset key or `alpha`; it will hand back the
//! corresponding *points*, through `GetPublicKey` and `GetPublicSpendKey`. So `offset = Σk_script − Σk_sender`
//! becomes `offset·G = ΣK_script − ΣK_sender`, and the alpha derived script key - which has no instruction of its
//! own - becomes `H(b)·G + alpha·G`, both halves of which the host can get. See
//! [`crate::fixtures::alpha_derived_script_public_key`].
//!
//! The one place this runs the other way is the shared secret. `k·P` cannot be checked from `K` and `P` alone -
//! that is the Diffie-Hellman problem - so the scenario supplies a point whose discrete log it chose, and checks
//! `k·P = p·K` instead.

use minotari_ledger_wallet_common::common_types::{Instruction, LedgerKeyBranch};
use minotari_ledger_wallet_comms::accessor_methods::{
    ledger_generate_ephemeral_nonce,
    ledger_get_dh_shared_secret,
    ledger_get_one_sided_metadata_signature,
    ledger_get_public_key,
    ledger_get_public_spend_key,
    ledger_get_raw_schnorr_signature,
    ledger_get_script_schnorr_signature,
};
use tari_crypto::{
    commitment::HomomorphicCommitment,
    keys::PublicKey,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    signatures::CommitmentAndPublicKeySignature,
};
use tari_utilities::hex::Hex;

use crate::{
    approver::{Outcome, while_reviewing},
    fixtures,
    raw::{self, ComAndPubSigReply, ScriptOffsetReply, payload},
    review::ExpectedReview,
    scenarios::{
        Approval,
        Scenario,
        ScenarioContext,
        ScenarioError,
        ScenarioModule,
        ScenarioResult,
        WithContext,
        expect_ok,
        require,
    },
};

pub const MODULE: ScenarioModule = ScenarioModule {
    name: "crypto",
    scenarios: SCENARIOS,
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "a script Schnorr signature verifies, over a nonce the device drew freshly each time",
        covers: &[Instruction::GetScriptSchnorrSignature, Instruction::GetPublicKey],
        approval: Approval::NotNeeded,
        run: script_schnorr_signature_verifies,
    },
    Scenario {
        name: "a raw Schnorr signature verifies against the reserved nonce and the named key",
        covers: &[
            Instruction::GenerateEphemeralNonce,
            Instruction::GetRawSchnorrSignature,
            Instruction::GetPublicKey,
        ],
        approval: Approval::NotNeeded,
        run: raw_schnorr_signature_verifies,
    },
    Scenario {
        name: "a managed script signature verifies against the indexed script key",
        covers: &[Instruction::GetScriptSignatureManaged, Instruction::GetPublicKey],
        approval: Approval::NotNeeded,
        run: managed_script_signature_verifies,
    },
    Scenario {
        name: "a derived script signature verifies against the alpha derived script key",
        covers: &[Instruction::GetScriptSignatureDerived, Instruction::GetPublicSpendKey],
        approval: Approval::NotNeeded,
        run: derived_script_signature_verifies,
    },
    Scenario {
        name: "a script offset is the script keys minus the sender offset keys, and only the device blinds it",
        covers: &[
            Instruction::GetScriptOffset,
            Instruction::GetPublicKey,
            Instruction::GetPublicSpendKey,
        ],
        approval: Approval::NotNeeded,
        run: the_script_offset_is_the_sum_it_claims,
    },
    Scenario {
        name: "a Diffie-Hellman shared secret is the device's key times the host's point",
        covers: &[Instruction::GetDHSharedSecret, Instruction::GetPublicKey],
        approval: Approval::NotNeeded,
        run: the_shared_secret_is_the_product,
    },
    Scenario {
        name: "an approved one sided metadata signature verifies against what the device showed",
        covers: &[Instruction::GetOneSidedMetadataSignature, Instruction::GetPublicKey],
        approval: Approval::Required,
        run: the_approved_metadata_signature_verifies,
    },
];

/// The network tag the script signature scenarios send.
///
/// Opaque here: the device only folds it into a hash label (`"script_challenge.n{network}"`), so any byte works as
/// long as the host uses the same one. A constant rather than a random draw so that a challenge mismatch is a
/// mismatch about the *construction* rather than about which byte went out.
const NETWORK: u8 = 0x02;

/// The transaction output/input version the script signature scenarios send. The device parses and then ignores
/// it - `finalize_script_signature_challenge` takes it as `_version` - so it is a constant.
const TXI_VERSION: u8 = 0;

/// Acceptance: `GetScriptSchnorrSignature` returns a signature that verifies against the key it was asked for, and
/// draws a fresh nonce every time.
///
/// Both halves matter and neither implies the other. A signature that verifies proves the device used the right
/// key; two signatures over the *same* message that differ prove it did not reuse the nonce - and nonce reuse
/// across two different messages gives up the private key outright, which is the failure this instruction's whole
/// design is arranged around. `verify_ledger_application` makes the second check too, on a key of its own choosing;
/// this makes it on a key the scenario named, and says so when it fails.
fn script_schnorr_signature_verifies(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let index = fixtures::random_u64();
    let branch = LedgerKeyBranch::OneSidedSenderOffset;
    let message = fixtures::random_bytes_32();

    let public_key = ledger_get_public_key(account, index, branch).context(|| "GetPublicKey".to_string())?;

    let first = ledger_get_script_schnorr_signature(account, index, branch, &message)
        .context(|| "GetScriptSchnorrSignature".to_string())?;
    let signature = first
        .to_schnorr_signature()
        .context(|| "the device's compressed signature would not decompress".to_string())?;
    require(signature.verify(&public_key, message), || {
        "the script Schnorr signature does not verify against the public key the device returned for the same account, \
         index and branch"
            .to_string()
    })?;

    let second = ledger_get_script_schnorr_signature(account, index, branch, &message)
        .context(|| "GetScriptSchnorrSignature, second call".to_string())?;
    require(first != second, || {
        "the device returned an identical signature twice over the same message, so it reused the nonce - two \
         signatures over different messages would then give up the private key"
            .to_string()
    })?;
    let second = second
        .to_schnorr_signature()
        .context(|| "the device's second compressed signature would not decompress".to_string())?;
    require(second.verify(&public_key, message), || {
        "the device's second script Schnorr signature does not verify".to_string()
    })
}

/// Acceptance: `GetRawSchnorrSignature` signs with the nonce it reserved and the key it was told to use.
///
/// The verification pins both. `s·G = R + e·P` holds only for the `R` the reservation returned and the `P` the
/// named branch and index derive to, so a device that signed with some other nonce - or quietly used a nonce of its
/// own instead of the reserved one - fails here rather than silently producing a signature nobody can use.
///
/// This is the instruction that replaced the legacy, host-indexed one; see [`super::legacy_nonce`] for what the
/// difference is worth.
fn raw_schnorr_signature_verifies(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let index = fixtures::random_u64();
    let branch = LedgerKeyBranch::PreMine;
    let challenge = fixtures::random_challenge();

    let public_key = ledger_get_public_key(account, index, branch).context(|| "GetPublicKey".to_string())?;
    let (handle, public_nonce) =
        ledger_generate_ephemeral_nonce(account).context(|| "GenerateEphemeralNonce".to_string())?;

    let signature = ledger_get_raw_schnorr_signature(account, index, branch, handle, &challenge)
        .context(|| "GetRawSchnorrSignature".to_string())?;

    require(signature.get_compressed_public_nonce() == &public_nonce, || {
        "the signature's public nonce is not the one GenerateEphemeralNonce handed back, so the device did not sign \
         with the nonce it reserved"
            .to_string()
    })?;

    let signature = signature
        .to_schnorr_signature()
        .context(|| "the device's compressed signature would not decompress".to_string())?;
    require(signature.verify_raw_uniform(&public_key, &challenge), || {
        "the raw Schnorr signature does not verify against the public key the device returned for the same account, \
         index and branch"
            .to_string()
    })
}

/// Acceptance: `GetScriptSignatureManaged` signs with the pre-mine script key the host named by index.
///
/// The commitment and public key signature is checked against the two things it is a statement about: the
/// commitment the scenario built - `commit(commitment_private_key, value)`, the device's own argument order - and
/// the script public key, which the host reads out of `GetPublicKey` for the same branch and index. Both come from
/// the device or from the scenario, never from a stored expectation.
///
/// Sent over raw APDUs rather than through `ledger_get_script_signature`, for one reason: the accessor takes a
/// `tari_common::Network`, and this crate deliberately does not depend on `tari_common` - see the comment on
/// `tari_common_types` in `Cargo.toml`. The network is an opaque byte in the challenge either way.
fn managed_script_signature_verifies(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let index = fixtures::random_u64();
    let branch = LedgerKeyBranch::PreMine;

    let script_public_key =
        ledger_get_public_key(account, index, branch).context(|| "GetPublicKey for the script key".to_string())?;

    let value = fixtures::random_secret_key();
    let commitment_private_key = fixtures::random_secret_key();
    let commitment = fixtures::script_commitment(&commitment_private_key, &value);
    let message = fixtures::random_bytes_32();

    let reply = raw::command(
        account,
        Instruction::GetScriptSignatureManaged,
        payload::script_signature_managed(
            NETWORK,
            TXI_VERSION,
            &fixtures::secret_key_bytes(&value),
            &fixtures::secret_key_bytes(&commitment_private_key),
            &fixtures::public_key_bytes(commitment.as_public_key()),
            &message,
            branch,
            index,
        ),
    )
    .send()
    .context(|| "GetScriptSignatureManaged".to_string())?;
    expect_ok("GetScriptSignatureManaged", &reply)?;

    verify_com_and_pub_signature(
        "the managed script signature",
        &reply.data,
        &commitment,
        &script_public_key,
        |ephemeral_commitment, ephemeral_pubkey| {
            fixtures::script_signature_challenge(
                NETWORK,
                ephemeral_commitment,
                ephemeral_pubkey,
                &script_public_key,
                &commitment,
                &message,
            )
        },
    )
}

/// Acceptance: `GetScriptSignatureDerived` signs with `H("script key", b) + alpha` for the blinding factor `b` the
/// host sent.
///
/// The script key here is one no instruction will hand over, not even in public form: it is derived from the
/// wallet's root spend key. The host can still name the point, because `alpha_hasher` is an *addition* -
/// `H(b) + alpha` - so its public key is `H(b)·G` plus the `alpha·G` that `GetPublicSpendKey` returns. Verifying
/// against that point is therefore a statement that the device folded the host's blinding factor into the right
/// root key, which is exactly what makes a derived script key recoverable by a wallet that has the seed.
fn derived_script_signature_verifies(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let blinding_factor = fixtures::random_secret_key();

    let public_spend_key = ledger_get_public_spend_key(account).context(|| "GetPublicSpendKey".to_string())?;
    let public_spend_key = public_spend_key
        .to_public_key()
        .context(|| "the device's public spend key would not decompress".to_string())?;
    let script_public_key = fixtures::alpha_derived_script_public_key(&blinding_factor, &public_spend_key);

    let value = fixtures::random_secret_key();
    let commitment_private_key = fixtures::random_secret_key();
    let commitment = fixtures::script_commitment(&commitment_private_key, &value);
    let message = fixtures::random_bytes_32();

    let reply = raw::command(
        account,
        Instruction::GetScriptSignatureDerived,
        payload::script_signature_derived(
            NETWORK,
            TXI_VERSION,
            &fixtures::secret_key_bytes(&value),
            &fixtures::secret_key_bytes(&commitment_private_key),
            &fixtures::public_key_bytes(commitment.as_public_key()),
            &message,
            &fixtures::secret_key_bytes(&blinding_factor),
        ),
    )
    .send()
    .context(|| "GetScriptSignatureDerived".to_string())?;
    expect_ok("GetScriptSignatureDerived", &reply)?;

    verify_com_and_pub_signature(
        "the derived script signature",
        &reply.data,
        &commitment,
        &script_public_key,
        |ephemeral_commitment, ephemeral_pubkey| {
            fixtures::script_signature_challenge(
                NETWORK,
                ephemeral_commitment,
                ephemeral_pubkey,
                &script_public_key,
                &commitment,
                &message,
            )
        },
    )
}

/// How many sender offset keys the script offset scenario asks the device to generate.
const SENDER_OFFSET_KEYS: u64 = 3;
/// How many pre-mine script keys it names by index.
const SCRIPT_INDEXES: u64 = 2;
/// How many alpha derived script keys it sends blinding factors for.
const DERIVED_SCRIPT_KEYS: u64 = 1;

/// Acceptance: a script offset really is `Σ script keys − Σ sender offset keys`, in the group.
///
/// This is the instruction with the most to get wrong and the worst consequence for getting it wrong, so the
/// assertion is the full sum rather than a smoke test. It covers every term at once:
///
/// * the two **pre-mine** script keys, which the device derives from indexes the host named;
/// * the one **alpha derived** script key, which the device folds into the wallet's root spend key and which the host
///   can therefore name only as a point (see [`derived_script_signature_verifies`]);
/// * the host's **partial sum**, which must be added and must not displace anything;
/// * the `SENDER_OFFSET_KEYS` sender offset keys the device generated from a base index it chose, walked with
///   `sender_offset_index` so that a base near the end of `u64` wraps identically on both sides.
///
/// An implementation that dropped a term, added instead of subtracting, or let the partial sum overwrite the
/// running total would fail here - and the last of those is not hypothetical: it would make the reply
/// `partial − k_sender`, from which the host recovers a sender offset private key by subtraction.
///
/// Sent over raw APDUs because the chunk numbering is the point of several sibling scenarios in
/// [`super::stateful`], and building both shapes the same way keeps them comparable.
fn the_script_offset_is_the_sum_it_claims(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let partial_sum = fixtures::random_secret_key();
    let script_indexes: Vec<u64> = (0..SCRIPT_INDEXES).map(|_| fixtures::random_u64()).collect();
    let blinding_factor = fixtures::random_secret_key();

    let mut chunks: Vec<Vec<u8>> = vec![
        payload::script_offset_header(SENDER_OFFSET_KEYS, SCRIPT_INDEXES, DERIVED_SCRIPT_KEYS),
        payload::script_offset_partial_sum(&fixtures::secret_key_bytes(&partial_sum)),
    ];
    for index in &script_indexes {
        chunks.push(payload::script_offset_script_index(LedgerKeyBranch::PreMine, *index));
    }
    chunks.push(payload::script_offset_derived_script_key(&fixtures::secret_key_bytes(
        &blinding_factor,
    )));

    let reply = send_script_offset_chunks(account, &chunks)?;
    expect_ok("GetScriptOffset", &reply)?;
    let parsed = ScriptOffsetReply::parse(&reply.data).map_err(super::fail)?;
    let script_offset =
        fixtures::secret_key_from_bytes("the script offset", &parsed.script_offset).map_err(super::fail)?;

    // The script side of the sum, as points.
    let public_spend_key = ledger_get_public_spend_key(account)
        .context(|| "GetPublicSpendKey".to_string())?
        .to_public_key()
        .context(|| "the device's public spend key would not decompress".to_string())?;
    let mut expected = fixtures::alpha_derived_script_public_key(&blinding_factor, &public_spend_key);
    expected = expected + RistrettoPublicKey::from_secret_key(&partial_sum);
    for index in &script_indexes {
        let key = ledger_get_public_key(account, *index, LedgerKeyBranch::PreMine)
            .context(|| format!("GetPublicKey for the pre-mine script key at index {index}"))?;
        expected = expected + key;
    }

    // ...minus the sender offset side, which the device chose the indexes for and told the host about.
    for i in 0..SENDER_OFFSET_KEYS {
        let index = minotari_ledger_wallet_common::script_offset::sender_offset_index(parsed.base_index, i);
        let key = ledger_get_public_key(account, index, LedgerKeyBranch::OneSidedSenderOffset)
            .context(|| format!("GetPublicKey for the sender offset key at index {index}"))?;
        expected = expected - key;
    }

    require(RistrettoPublicKey::from_secret_key(&script_offset) == expected, || {
        format!(
            "the script offset is not the sum it claims to be. The device returned a scalar whose public key is {}, \
             but the script keys minus the sender offset keys the device itself named come to {}. Base index {}, \
             {SCRIPT_INDEXES} pre-mine script keys, {DERIVED_SCRIPT_KEYS} derived script key, {SENDER_OFFSET_KEYS} \
             sender offset keys.",
            RistrettoPublicKey::from_secret_key(&script_offset).to_hex(),
            expected.to_hex(),
            parsed.base_index
        )
    })
}

/// Acceptance: `GetDHSharedSecret` returns `k·P` for the device's key `k` and the host's point `P`.
///
/// Checked the only way it can be. `k·P` cannot be derived from `K = k·G` and `P` - that is the Diffie-Hellman
/// problem, and if it could the instruction would be pointless - so the scenario picks `P = p·G` for a scalar `p`
/// it drew itself, and checks the device's answer against `p·K`. Both sides equal `k·p·G`, so the equality holds
/// exactly when the device multiplied the right key by the right point.
///
/// A device that returned `K` itself, or `P` itself, or a hash of the two, fails this.
fn the_shared_secret_is_the_product(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let index = fixtures::random_u64();
    let branch = LedgerKeyBranch::OneSidedSenderOffset;

    let host_secret = fixtures::random_secret_key();
    let host_point = RistrettoPublicKey::from_secret_key(&host_secret);

    let device_key = ledger_get_public_key(account, index, branch).context(|| "GetPublicKey".to_string())?;
    let shared = ledger_get_dh_shared_secret(
        account,
        index,
        branch,
        &tari_common_types::types::CompressedPublicKey::new_from_pk(host_point),
    )
    .context(|| "GetDHSharedSecret".to_string())?;
    let shared = shared
        .to_public_key()
        .context(|| "the device's shared secret would not decompress".to_string())?;

    let expected = host_secret * device_key.clone();
    require(shared == expected, || {
        format!(
            "the shared secret is {} but the device's key {} times the host's own scalar is {}",
            shared.to_hex(),
            device_key.to_hex(),
            expected.to_hex()
        )
    })
}

/// The amount the review shows. Under a million, so the device renders it in microTari - see
/// [`crate::review::minotari_amount`], which has to make the same choice.
const REVIEW_VALUE: u64 = 12_345;

/// Acceptance: the device signs what it showed, and the signature is a valid metadata signature.
///
/// **The one scenario in this suite that needs approval**, on either frontend - on the simulator the buttons are
/// pressed by [`crate::approver::SpeculosApprover`], on hardware by a human. That is affordable because
/// `GetOneSidedMetadataSignature` is the only handler in the whole application that puts anything on the screen.
///
/// Two independent statements, and the second is the one no other test in this repository makes:
///
/// 1. The **screen** showed the amount and the receiver the host asked for, and no `Payment ID` row it did not. That is
///    [`ExpectedReview`]'s doing, and on hardware the operator is the oracle.
/// 2. The **signature** the device then produced is over the sender offset key it was asked for, the commitment the
///    host built, and a message derived from the receiver's own stealth script. Which is to say: what the user approved
///    on screen and what the device signed are the same transaction. A device that displayed one address and signed for
///    another would pass every screen assertion ever written, and fails here.
fn the_approved_metadata_signature_verifies(context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let sender_offset_key_index = fixtures::random_u64();
    let commitment_mask = fixtures::random_secret_key();
    let common_message = fixtures::random_bytes_32();
    let receiver = fixtures::published_receiver(0).map_err(super::fail)?;

    let sender_offset_public_key =
        ledger_get_public_key(account, sender_offset_key_index, LedgerKeyBranch::OneSidedSenderOffset)
            .context(|| "GetPublicKey for the sender offset key".to_string())?;

    let expected_review = ExpectedReview::one_sided_metadata_signature(REVIEW_VALUE, &receiver.to_base58(), 0);
    let (signature, review) = while_reviewing(context.approver(), &expected_review, Outcome::Approve, || {
        ledger_get_one_sided_metadata_signature(
            account,
            receiver.network(),
            0,
            REVIEW_VALUE,
            sender_offset_key_index,
            &commitment_mask,
            &receiver,
            &common_message,
        )
    });
    review.context(|| "the device's review screen".to_string())?;
    let signature = signature.context(|| "GetOneSidedMetadataSignature".to_string())?;

    // What the device should have signed, rebuilt from the same inputs the request carried.
    let value = RistrettoSecretKey::from(REVIEW_VALUE);
    let commitment = fixtures::script_commitment(&commitment_mask, &value);
    let receiver_spend_key = receiver
        .public_spend_key()
        .to_public_key()
        .context(|| "the published receiver address's spend key would not decompress".to_string())?;
    let script = fixtures::stealth_script(&commitment_mask, &receiver_spend_key);
    let network = receiver.network().as_byte();
    let message = fixtures::metadata_signature_message(network, &script, &common_message);

    verify_com_and_pub_signature_body(
        "the one sided metadata signature",
        &signature.to_vec(),
        &commitment,
        &sender_offset_public_key,
        |ephemeral_commitment, ephemeral_pubkey| {
            fixtures::metadata_signature_challenge(
                network,
                &sender_offset_public_key,
                ephemeral_commitment,
                ephemeral_pubkey,
                &commitment,
                &message,
            )
        },
    )
}

/// Send a `GetScriptOffset` exchange as a well formed 0, 1, 2, ... sequence and return the last reply.
///
/// The continuation flag is set on every chunk but the last, which is what `chunk_command` does. Malformed
/// sequences are [`super::stateful`]'s business and build their chunks one at a time.
fn send_script_offset_chunks(account: u64, chunks: &[Vec<u8>]) -> Result<crate::raw::RawReply, ScenarioError> {
    let last = chunks.len().saturating_sub(1);
    let mut reply = None;
    for (number, chunk) in chunks.iter().enumerate() {
        let number = u8::try_from(number)
            .map_err(|_| super::fail("a script offset scenario asked for more chunks than a u8 can number"))?;
        reply = Some(
            raw::chunk(
                account,
                Instruction::GetScriptOffset,
                number,
                usize::from(number) != last,
                chunk.clone(),
            )
            .send()
            .context(|| format!("GetScriptOffset chunk {number}"))?,
        );
    }
    reply.ok_or_else(|| super::fail("a script offset scenario sent no chunks at all"))
}

/// Parse a `version(1) | 160 byte` signature reply and verify it against a challenge the caller builds from the
/// ephemeral values the device chose.
///
/// The challenge cannot be built before the exchange, because two of its inputs - the ephemeral commitment and the
/// ephemeral public key - are nonces the device draws. So the caller passes the rest of the challenge in as a
/// closure and this fills in the two it just read off the wire. That structure is also what makes the assertion
/// meaningful: the device's own nonces go into the challenge its own signature has to satisfy, so there is no
/// value here that both sides could be wrong about in the same way.
fn verify_com_and_pub_signature(
    what: &str,
    reply_data: &[u8],
    commitment: &HomomorphicCommitment<RistrettoPublicKey>,
    public_key: &RistrettoPublicKey,
    challenge: impl FnOnce(&HomomorphicCommitment<RistrettoPublicKey>, &RistrettoPublicKey) -> [u8; 64],
) -> ScenarioResult {
    let body = reply_data
        .get(1..)
        .ok_or_else(|| super::fail(format!("{what} reply is empty")))?;
    verify_com_and_pub_signature_body(what, body, commitment, public_key, challenge)
}

/// [`verify_com_and_pub_signature`] without the leading response version byte, for a signature that has already
/// been parsed by an accessor method and handed back as a type rather than as a reply.
fn verify_com_and_pub_signature_body(
    what: &str,
    body: &[u8],
    commitment: &HomomorphicCommitment<RistrettoPublicKey>,
    public_key: &RistrettoPublicKey,
    challenge: impl FnOnce(&HomomorphicCommitment<RistrettoPublicKey>, &RistrettoPublicKey) -> [u8; 64],
) -> ScenarioResult {
    let parsed = ComAndPubSigReply::parse_body(what, body).map_err(super::fail)?;
    let ephemeral_commitment = HomomorphicCommitment::from_public_key(
        &fixtures::public_key_from_bytes("the ephemeral commitment", &parsed.ephemeral_commitment)
            .map_err(super::fail)?,
    );
    let ephemeral_pubkey =
        fixtures::public_key_from_bytes("the ephemeral public key", &parsed.ephemeral_pubkey).map_err(super::fail)?;
    let u_a = fixtures::secret_key_from_bytes("u_a", &parsed.u_a).map_err(super::fail)?;
    let u_x = fixtures::secret_key_from_bytes("u_x", &parsed.u_x).map_err(super::fail)?;
    let u_y = fixtures::secret_key_from_bytes("u_y", &parsed.u_y).map_err(super::fail)?;

    let challenge = challenge(&ephemeral_commitment, &ephemeral_pubkey);
    let signature = CommitmentAndPublicKeySignature::new(ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y);

    require(
        signature.verify_challenge(
            commitment,
            public_key,
            &challenge,
            &fixtures::commitment_factory(),
            &mut rand::rng(),
        ),
        || {
            format!(
                "{what} does not verify. It should be a signature over the commitment {} and the public key {}, with \
                 the challenge the device's own ephemeral values imply.",
                commitment.as_public_key().to_hex(),
                public_key.to_hex()
            )
        },
    )
}
