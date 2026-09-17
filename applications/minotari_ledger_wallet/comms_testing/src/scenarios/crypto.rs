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
    ScriptSignatureKey,
    ledger_generate_ephemeral_nonce,
    ledger_get_dh_shared_secret,
    ledger_get_one_sided_metadata_signature,
    ledger_get_public_key,
    ledger_get_public_spend_key,
    ledger_get_raw_schnorr_signature,
    ledger_get_script_offset,
    ledger_get_script_schnorr_signature,
    ledger_get_script_signature,
};
use tari_common::configuration::Network;
use tari_common_types::types::CompressedCommitment;
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
    raw::ComAndPubSigReply,
    review::ExpectedReview,
    scenarios::{Approval, Scenario, ScenarioContext, ScenarioModule, ScenarioResult, WithContext, require},
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

/// The networks the managed script signature scenario signs under.
///
/// The device folds `as_byte()` into a hash label (`"script_challenge.n{network}"`) and does nothing else with it,
/// so the property worth checking is that the byte the host puts in its challenge is the byte the device put in
/// its own. A single value cannot show that: with one network on both sides, a device that ignored the field
/// entirely would agree just as well.
///
/// So the managed scenario signs under two, and they must produce *different* signatures over otherwise identical
/// inputs. Two rather than a random draw, because a network is a small closed set and a mismatch should be a
/// mismatch about the construction rather than about which byte happened to go out. Neither belongs on a real
/// chain.
const NETWORKS: [Network; 2] = [Network::LocalNet, Network::NextNet];

/// The network the other signature scenarios sign under, where the point is the key rather than the label.
const NETWORK: Network = Network::LocalNet;

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
/// # Through `ledger_get_script_signature`, not over raw APDUs
///
/// Every **happy path** in this suite goes through the accessor method the console wallet actually calls, and that
/// is a deliberate division of labour with [`crate::raw`]:
///
/// * a **rejection** scenario has to use raw APDUs, because the accessors mirror the device's rules and refuse the
///   malformed request before it reaches the wire - a probe driven through one would test the mirror;
/// * a **happy path** has to use the accessor, because the accessor is shipped code. It lays out the payload, picks the
///   instruction from the [`ScriptSignatureKey`] variant, and parses the 161 byte reply. A re-implementation of that in
///   the test suite would stay green while the shipped one regressed, which is the one failure a device suite most
///   needs not to have.
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

    // Once per network, with everything else held fixed. See `NETWORKS`: one value cannot show that the device
    // put the host's network byte into its own challenge, because a device that ignored the field would agree.
    let mut responses = Vec::with_capacity(NETWORKS.len());
    for network in NETWORKS {
        let signature = ledger_get_script_signature(
            account,
            network,
            TXI_VERSION,
            &ScriptSignatureKey::Managed { branch, index },
            &value,
            &commitment_private_key,
            &CompressedCommitment::from_commitment(commitment.clone()),
            message,
        )
        .context(|| format!("GetScriptSignature (managed) on {network}"))?;

        verify_com_and_pub_signature(
            &format!("the managed script signature on {network}"),
            &signature.to_vec(),
            &commitment,
            &script_public_key,
            |ephemeral_commitment, ephemeral_pubkey| {
                fixtures::script_signature_challenge(
                    network.as_byte(),
                    ephemeral_commitment,
                    ephemeral_pubkey,
                    &script_public_key,
                    &commitment,
                    &message,
                )
            },
        )?;
        responses.push((network, signature.u_x().clone()));
    }

    // Each verified against its own network's challenge above, so a device that ignored the network would have
    // had to satisfy two different challenges with one construction. `u_x` is compared rather than the whole
    // signature because the ephemeral values are fresh nonces and would differ regardless - it is the response
    // scalar, which is `r_x + e * x`, and `e` is the only thing the network moves.
    let differ = match responses.as_slice() {
        [(_, first), (_, second)] => first != second,
        _ => return Err(super::fail("expected exactly one response per network")),
    };
    require(differ, || {
        format!(
            "signing the same script signature under {} and {} produced the same response scalar, so the network \
             never reached the device's challenge",
            NETWORKS[0], NETWORKS[1]
        )
    })
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

    let signature = ledger_get_script_signature(
        account,
        NETWORK,
        TXI_VERSION,
        // The accessor sends this key as the blinding factor, and picks `GetScriptSignatureDerived` off the
        // variant. Driving it from here rather than building the payload is what keeps that branch of shipped code
        // exercised.
        &ScriptSignatureKey::Derived {
            branch_key: blinding_factor.clone(),
        },
        &value,
        &commitment_private_key,
        &CompressedCommitment::from_commitment(commitment.clone()),
        message,
    )
    .context(|| "GetScriptSignature (derived)".to_string())?;

    verify_com_and_pub_signature(
        "the derived script signature",
        &signature.to_vec(),
        &commitment,
        &script_public_key,
        |ephemeral_commitment, ephemeral_pubkey| {
            fixtures::script_signature_challenge(
                NETWORK.as_byte(),
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
/// # Through `ledger_get_script_offset`, which is the only test of the shipped chunking
///
/// This instruction is the one whose host side is more than a payload layout: `ledger_get_script_offset` decides
/// how many chunks there are, what goes in each, which one carries the account, which one sets the continuation
/// flag, and it walks `sender_offset_index` to turn the base index the device returns back into the list of key
/// indexes the caller gets. `chunk_command` is shipped code with real logic in it.
///
/// So the happy path is driven through the accessor, and the malformed sequences in [`super::stateful`] are driven
/// over raw APDUs - because those are shapes `chunk_command` cannot express and the accessor refuses before the
/// wire. Between them the shipped assembly and the device's tolerance of a hostile one are both covered; a suite
/// that re-implemented the chunking for its happy path would have left the shipped version untested.
///
/// The sender offset indexes are taken from the accessor's own return value rather than recomputed here. Be
/// precise about what that does and does not check: the accessor and the device call the *same*
/// `sender_offset_index` out of `minotari_ledger_wallet_common`, so a change to that function moves both sides
/// together and the group equation below still balances. What is under test is that the accessor keeps using the
/// shared walk rather than growing a private reimplementation of it - which is worth having, and is all it is.
///
/// The wrap in that walk is likewise not exercised. The base index is drawn by the device, so reaching a base
/// within `SENDER_OFFSET_KEYS` of `u64::MAX` has probability around 2^-62; `script_offset`'s own unit tests cover
/// the wrap directly, without a device.
fn the_script_offset_is_the_sum_it_claims(_context: &ScenarioContext<'_>) -> ScenarioResult {
    let account = fixtures::random_u64();
    let partial_sum = fixtures::random_secret_key();
    let script_indexes: Vec<(LedgerKeyBranch, u64)> = (0..SCRIPT_INDEXES)
        .map(|_| (LedgerKeyBranch::PreMine, fixtures::random_u64()))
        .collect();
    let derived_script_keys: Vec<RistrettoSecretKey> = (0..DERIVED_SCRIPT_KEYS)
        .map(|_| fixtures::random_secret_key())
        .collect();

    let sender_offset_count =
        usize::try_from(SENDER_OFFSET_KEYS).map_err(|_| super::fail("SENDER_OFFSET_KEYS does not fit in a usize"))?;
    let (script_offset, sender_offset_indexes) = ledger_get_script_offset(
        account,
        &partial_sum,
        &derived_script_keys,
        &script_indexes,
        sender_offset_count,
    )
    .context(|| "GetScriptOffset".to_string())?;

    require(sender_offset_indexes.len() == sender_offset_count, || {
        format!(
            "the accessor returned {} sender offset indexes for a request of {sender_offset_count}",
            sender_offset_indexes.len()
        )
    })?;

    // The script side of the sum, as points.
    let public_spend_key = ledger_get_public_spend_key(account)
        .context(|| "GetPublicSpendKey".to_string())?
        .to_public_key()
        .context(|| "the device's public spend key would not decompress".to_string())?;
    let mut expected = RistrettoPublicKey::from_secret_key(&partial_sum);
    for blinding_factor in &derived_script_keys {
        expected = expected + fixtures::alpha_derived_script_public_key(blinding_factor, &public_spend_key);
    }
    for (branch, index) in &script_indexes {
        let key = ledger_get_public_key(account, *index, *branch)
            .context(|| format!("GetPublicKey for the pre-mine script key at index {index}"))?;
        expected = expected + key;
    }

    // ...minus the sender offset side, at the indexes the accessor derived from the base the device chose.
    for index in &sender_offset_indexes {
        let key = ledger_get_public_key(account, *index, LedgerKeyBranch::OneSidedSenderOffset)
            .context(|| format!("GetPublicKey for the sender offset key at index {index}"))?;
        expected = expected - key;
    }

    require(RistrettoPublicKey::from_secret_key(&script_offset) == expected, || {
        format!(
            "the script offset is not the sum it claims to be. The device returned a scalar whose public key is {}, \
             but the script keys minus the sender offset keys the device itself named come to {}. Sender offset \
             indexes {sender_offset_indexes:?}, {SCRIPT_INDEXES} pre-mine script keys, {DERIVED_SCRIPT_KEYS} derived \
             script key.",
            RistrettoPublicKey::from_secret_key(&script_offset).to_hex(),
            expected.to_hex(),
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
///
/// That leaves the other branch of that function - `{:.2} T`, at or above one million - **never compared against a
/// device**. It is covered host side by `review::test::the_amount_is_formatted_the_way_the_device_formats_it`, so
/// the formatter is tested; what is untested is that the *device* agrees with it above a million. Closing that
/// would take a second approval scenario, which costs a human interaction on every hardware run and would break
/// the "exactly one approval scenario" invariant in `scenarios::mod`. Recorded rather than hidden.
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

    verify_com_and_pub_signature(
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

/// Verify a commitment and public key signature against a challenge the caller builds from the ephemeral values
/// the device chose.
///
/// Takes the 160 byte body rather than a reply, because every caller here drives an accessor method and so has a
/// parsed signature rather than bytes off the wire; `to_vec` on the compressed signature reproduces exactly those
/// 160 bytes, in `ComAndPubSigReply`'s field order.
///
/// The challenge cannot be built before the exchange, because two of its inputs - the ephemeral commitment and the
/// ephemeral public key - are nonces the device draws. So the caller passes the rest of the challenge in as a
/// closure and this fills in the two it just read back. That structure is also what makes the assertion
/// meaningful: the device's own nonces go into the challenge its own signature has to satisfy, so there is no
/// value here that both sides could be wrong about in the same way.
fn verify_com_and_pub_signature(
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
