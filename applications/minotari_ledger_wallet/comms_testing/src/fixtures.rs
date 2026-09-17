// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The inputs the scenarios send, and the host side arithmetic that checks what comes back.
//!
//! # Hand built, never through the key manager
//!
//! Nothing here uses `transaction_components::test_helpers`. Those helpers build their fixtures through the key
//! manager, and a key manager change would then turn this suite red - sending whoever is on the hook for it to
//! debug the one layer this suite exists to be independent of. A device test that fails because a host side key
//! manager moved has told you nothing about the device.
//!
//! So every value below is built directly from `tari_crypto`: a scalar is a scalar, a commitment is
//! `factory.commit(k, v)`, and a challenge is a hash this file writes out.
//!
//! # Random, not frozen
//!
//! Scenario inputs are drawn fresh on every run. A frozen input is a question the device has already answered,
//! and - worse - a device that ignores an argument passes a frozen input just as happily as a correct one. The
//! *assertions* are what is deterministic here: every one of them is an equation that holds for any input, which
//! is why a random input costs no reproducibility. Where a value has to be stable across two exchanges of one
//! scenario, the scenario draws it once and reuses it.
//!
//! The one thing that is emphatically not random is the vector table, which is Spec 2's and lives in
//! [`crate::vectors`].
//!
//! # The device's own crypto, restated rather than imported
//!
//! [`script_signature_challenge`] and [`alpha_derived_script_key`] are host side statements of two constructions
//! the device application performs. They are written out here rather than called, because the device's versions are
//! `no_std` code inside a binary this crate cannot link - and because an assertion that called the device's own
//! implementation would agree with it by construction and therefore assert nothing. Two independent statements of
//! one rule is the point, and it is the same argument [`crate::review::minotari_amount`] and [`crate::oracle`] are
//! built on.

use blake2::Blake2b;
use digest::{Digest, consts::U64};
use rand::Rng;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    hash_domain,
    hashing::{DomainSeparatedHasher, DomainSeparation},
    keys::{PublicKey, SecretKey},
    ristretto::{
        RistrettoPublicKey,
        RistrettoSecretKey,
        pedersen::{PedersenCommitment, commitment_factory::PedersenCommitmentFactory},
    },
};
use tari_utilities::ByteArray;

// The two hash domains the device application defines in `wallet/src/utils.rs`, restated here rather than
// imported - see the module docs. The domain string and the version are both part of the separation tag, so a
// typo in either produces a hash that is perfectly well formed and agrees with nothing.
hash_domain!(TransactionHashDomain, "com.tari.base_layer.core.transactions", 0);
hash_domain!(
    KeyManagerTransactionsHashDomain,
    "com.tari.base_layer.core.transactions.key_manager",
    1
);

/// A uniformly random scalar.
///
/// Drawn by wide reduction from 64 random bytes, which is how both the device and `ledger_demo` draw theirs, so a
/// scalar this produces is indistinguishable from one the wallet would have sent.
pub fn random_secret_key() -> RistrettoSecretKey {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    RistrettoSecretKey::from_uniform_bytes(&bytes).expect("64 uniform bytes always reduce onto a scalar")
}

/// A random scalar, as the 32 canonical bytes a payload carries.
pub fn random_scalar_bytes() -> [u8; 32] {
    to_array_32(random_secret_key().as_bytes())
}

/// 32 random bytes, for a message or a hash sized field that is not a scalar.
pub fn random_bytes_32() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

/// 64 random bytes, for a `GetRawSchnorrSignature` challenge.
///
/// A challenge is reduced by wide reduction on the device, so any 64 bytes are a valid challenge - there is no
/// shape a random draw could miss.
pub fn random_challenge() -> [u8; 64] {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

/// A random `u64`, for an account or a key index.
pub fn random_u64() -> u64 {
    rand::rng().next_u64()
}

/// The 32 canonical bytes of a public key.
pub fn public_key_bytes(key: &RistrettoPublicKey) -> [u8; 32] {
    to_array_32(key.as_bytes())
}

/// The 32 canonical bytes of a secret key.
pub fn secret_key_bytes(key: &RistrettoSecretKey) -> [u8; 32] {
    to_array_32(key.as_bytes())
}

/// Read a public key back out of the 32 bytes the device sent.
pub fn public_key_from_bytes(what: &str, bytes: &[u8]) -> Result<RistrettoPublicKey, String> {
    RistrettoPublicKey::from_canonical_bytes(bytes)
        .map_err(|e| format!("{what} is not a canonical Ristretto point: {e}"))
}

/// Read a secret key back out of the 32 bytes the device sent.
pub fn secret_key_from_bytes(what: &str, bytes: &[u8]) -> Result<RistrettoSecretKey, String> {
    RistrettoSecretKey::from_canonical_bytes(bytes)
        .map_err(|e| format!("{what} is not a canonical Ristretto scalar: {e}"))
}

/// The commitment factory the device commits with: `PedersenCommitmentFactory::default()`.
///
/// `wallet/src/crypto/commitment_factory.rs` is a copy of this one, down to the transcribed `TARI_H` bytes, so the
/// generators are the same pair and a commitment built here opens the one the device built.
pub fn commitment_factory() -> PedersenCommitmentFactory {
    PedersenCommitmentFactory::default()
}

/// The commitment a script signature is a statement about: `commit(commitment_private_key, value)`.
///
/// The argument order is `commit(k, v)` - blinding factor then value - and it matters. The device signs with
/// `CommitmentAndPublicKeySignature::sign(&value, &commitment_private_key, ..)`, whose `(a, x)` are value and
/// blinding factor in that order, and whose own ephemeral commitment is `commit(r_x, r_a)`. Swapping the two here
/// produces a commitment that no correct signature verifies against.
pub fn script_commitment(
    commitment_private_key: &RistrettoSecretKey,
    value: &RistrettoSecretKey,
) -> PedersenCommitment {
    commitment_factory().commit(commitment_private_key, value)
}

/// The challenge the device signs a script signature over.
///
/// A host side statement of `finalize_script_signature_challenge` in
/// `wallet/src/handlers/get_script_signature.rs`, which is
/// `DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("script_challenge", network)`
/// chained with the ephemeral commitment, the ephemeral public key, the script public key, the commitment and the
/// message.
///
/// # The encoding is borsh, and borsh is not "the bytes"
///
/// That hasher is borsh backed, and the device's keys serialise as `BorshSerialize::serialize(&self.as_bytes())` -
/// `as_bytes` returns a **slice**, so borsh writes a `u32` little endian length before the 32 bytes. The message
/// is a `[u8; 32]`, a borsh *array*, which carries no length prefix at all. Those two encodings sitting next to
/// each other is not a mistake in the device; it is what borsh does with a slice and with an array, and a
/// reimplementation that treated them alike would produce a challenge that nothing verifies against.
///
/// The network is folded into the label rather than hashed: `format!("{label}.n{network}")`, from
/// `DomainSeparatedConsensusHasher::new`.
pub fn script_signature_challenge(
    network: u8,
    ephemeral_commitment: &PedersenCommitment,
    ephemeral_pubkey: &RistrettoPublicKey,
    script_public_key: &RistrettoPublicKey,
    commitment: &PedersenCommitment,
    message: &[u8; 32],
) -> [u8; 64] {
    let mut digest = Blake2b::<U64>::default();
    TransactionHashDomain::add_domain_separation_tag(&mut digest, format!("script_challenge.n{network}"));
    // Four borsh slices, each a `u32` length then the bytes...
    for key in [
        ephemeral_commitment.as_public_key().as_bytes(),
        ephemeral_pubkey.as_bytes(),
        script_public_key.as_bytes(),
        commitment.as_public_key().as_bytes(),
    ] {
        let length = u32::try_from(key.len()).unwrap_or_default();
        Digest::update(&mut digest, length.to_le_bytes());
        Digest::update(&mut digest, key);
    }
    // ...then one borsh array, with no length at all.
    Digest::update(&mut digest, message);

    let mut challenge = [0u8; 64];
    challenge.copy_from_slice(digest.finalize().as_slice());
    challenge
}

/// The scalar the device adds to `alpha` for an alpha derived script key.
///
/// A host side statement of `alpha_hasher` in `wallet/src/utils.rs`:
/// `H("script key", blinding_factor)`, wide reduced. The device then returns `that + alpha`, so the corresponding
/// public key is [`alpha_derived_script_public_key`] - which the host can compute without ever seeing `alpha`,
/// because `GetPublicSpendKey` hands back `alpha * G`.
pub fn alpha_derived_script_key(blinding_factor: &RistrettoSecretKey) -> RistrettoSecretKey {
    let hashed = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label("script key")
        .chain(blinding_factor.as_bytes())
        .finalize();
    RistrettoSecretKey::from_uniform_bytes(hashed.as_ref()).expect("a 64 byte hash always reduces onto a scalar")
}

/// The public form of the script key the device derives from `blinding_factor`, given the account's `alpha * G`.
///
/// `alpha_hasher` returns `H(blinding_factor) + alpha`, so its public key is `H(blinding_factor) * G + alpha * G`.
/// The second term is exactly what `GetPublicSpendKey` returns, which is why a derived script key can be checked
/// from the host without the wallet's spend key ever leaving the device.
pub fn alpha_derived_script_public_key(
    blinding_factor: &RistrettoSecretKey,
    public_spend_key: &RistrettoPublicKey,
) -> RistrettoPublicKey {
    RistrettoPublicKey::from_secret_key(&alpha_derived_script_key(blinding_factor)) + public_spend_key.clone()
}

/// A published Tari dual address, and the one every review scenario sends.
///
/// It only has to be a well formed address whose public spend key is a real Ristretto point: the device parses it
/// after the review is approved and fails the instruction if it is not, which would turn a screen assertion into a
/// confusing signing error.
///
/// It lives here rather than in each scenario so that the simulator scenarios, the hardware frontend and
/// `examples/human_review.rs` all review the *same* transaction. Two copies of an address constant is two
/// transactions that look identical in a report and are not.
pub const RECEIVER_BASE58: &str =
    "f48ScXDKxTU3nCQsQrXHs4tnkAyLViSUpi21t7YuBNsJE1VpqFcNSeEzQWgNeCqnpRaCA9xRZ3VuV11F8pHyciegbCt";

/// [`RECEIVER_BASE58`], optionally carrying `payment_id_length` bytes of payment ID.
///
/// The payment ID variant is built from the published address's own keys rather than from fresh random ones, so
/// that two scenarios differ in exactly the thing under test - whether a `Payment ID` row appears on the device -
/// and not also in which keys are involved.
pub fn published_receiver(payment_id_length: usize) -> Result<tari_common_types::tari_address::TariAddress, String> {
    use tari_common_types::tari_address::{TariAddress, TariAddressFeatures};

    let published = TariAddress::from_base58(RECEIVER_BASE58).map_err(|e| format!("{e}"))?;
    if payment_id_length == 0 {
        return Ok(published);
    }
    let view_key = published
        .public_view_key()
        .ok_or_else(|| "a dual address must have a view key".to_string())?
        .clone();
    TariAddress::new_dual_address(
        view_key,
        published.public_spend_key().clone(),
        published.network(),
        // `new_dual_address` sets `PAYMENT_ID` itself when it is given payment ID bytes, but saying so here keeps
        // the intent of the caller in the caller.
        TariAddressFeatures::default() | TariAddressFeatures::PAYMENT_ID,
        Some(vec![0xAB; payment_id_length]),
    )
    .map_err(|e| format!("{e}"))
}

/// The one sided stealth script the device builds for a receiver, as the bytes it hashes.
///
/// A host side statement of `tari_script_with_address` in
/// `wallet/src/handlers/get_one_sided_metadata_signature.rs`: `PushPubKey(receiver_spend_key +
/// H("script key", commitment_mask) * G)`, serialised as `length(33) | opcode(0x7e) | key(32)`.
///
/// The hash is the same `H("script key", ...)` that [`alpha_derived_script_key`] uses, with the commitment mask in
/// place of a blinding factor - which is not a coincidence, it is the same stealth address construction applied to
/// a different secret, and sharing the function here is what keeps the two from drifting.
///
/// Those 35 bytes reach the hasher raw, with no length prefix: the device's `Script` borsh implementation writes
/// each `u8` individually rather than serialising the `Vec`, so borsh's usual slice length prefix never appears.
pub fn stealth_script(commitment_mask: &RistrettoSecretKey, receiver_public_spend_key: &RistrettoPublicKey) -> Vec<u8> {
    let stealth_key = receiver_public_spend_key.clone() +
        RistrettoPublicKey::from_secret_key(&alpha_derived_script_key(commitment_mask));
    let mut script = Vec::with_capacity(34);
    script.push(33); // length
    script.push(0x7e); // PushPubKey
    script.extend_from_slice(stealth_key.as_bytes());
    script
}

/// The 32 byte message a metadata signature is over.
///
/// A host side statement of `metadata_signature_message_from_script_and_common`: a **Blake2b-256** consensus hash
/// of the script and the caller's common message. Note the digest width - the challenge below is 512 bit and this
/// is 256 bit, and swapping them produces something that hashes cleanly and verifies against nothing.
pub fn metadata_signature_message(network: u8, script: &[u8], common: &[u8; 32]) -> [u8; 32] {
    let mut digest = blake2::Blake2b::<digest::consts::U32>::default();
    TransactionHashDomain::add_domain_separation_tag(&mut digest, format!("metadata_message.n{network}"));
    // Both borsh arrays - the script because the device serialises it byte by byte, the common message because it
    // is a `[u8; 32]` - so neither carries a length prefix.
    Digest::update(&mut digest, script);
    Digest::update(&mut digest, common);

    let mut message = [0u8; 32];
    message.copy_from_slice(digest.finalize().as_slice());
    message
}

/// The challenge the device signs a one sided metadata signature over.
///
/// A host side statement of `finalize_metadata_signature_challenge`. Same borsh encoding rules as
/// [`script_signature_challenge`] - four length prefixed keys then one bare array - but **a different field
/// order**: ephemeral public key, ephemeral commitment, sender offset public key, commitment, message. The script
/// signature challenge starts with the ephemeral *commitment*. Copying one from the other is the obvious way to get
/// this wrong, and the resulting challenge verifies against nothing.
pub fn metadata_signature_challenge(
    network: u8,
    sender_offset_public_key: &RistrettoPublicKey,
    ephemeral_commitment: &PedersenCommitment,
    ephemeral_pubkey: &RistrettoPublicKey,
    commitment: &PedersenCommitment,
    message: &[u8; 32],
) -> [u8; 64] {
    let mut digest = Blake2b::<U64>::default();
    TransactionHashDomain::add_domain_separation_tag(&mut digest, format!("metadata_signature.n{network}"));
    for key in [
        ephemeral_pubkey.as_bytes(),
        ephemeral_commitment.as_public_key().as_bytes(),
        sender_offset_public_key.as_bytes(),
        commitment.as_public_key().as_bytes(),
    ] {
        let length = u32::try_from(key.len()).unwrap_or_default();
        Digest::update(&mut digest, length.to_le_bytes());
        Digest::update(&mut digest, key);
    }
    Digest::update(&mut digest, message);

    let mut challenge = [0u8; 64];
    challenge.copy_from_slice(digest.finalize().as_slice());
    challenge
}

fn to_array_32(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.get(..32).unwrap_or_default());
    out
}

#[cfg(test)]
mod test {
    use tari_crypto::ristretto::RistrettoComAndPubSig;

    use super::*;

    /// Two draws must differ. A fixture generator that returned a constant would make several scenarios - notably
    /// "two signatures over the same key differ" - pass without the device doing anything.
    #[test]
    fn random_fixtures_are_actually_random() {
        assert_ne!(random_secret_key(), random_secret_key());
        assert_ne!(random_bytes_32(), random_bytes_32());
        assert_ne!(random_challenge().to_vec(), random_challenge().to_vec());
        assert_ne!(random_u64(), random_u64());
    }

    /// The round trip through the 32 byte wire form is lossless, which every scenario that sends a key and then
    /// checks the reply against it depends on.
    #[test]
    fn keys_round_trip_through_their_wire_bytes() {
        let secret = random_secret_key();
        let public = RistrettoPublicKey::from_secret_key(&secret);
        assert_eq!(
            secret_key_from_bytes("secret", &secret_key_bytes(&secret)).unwrap(),
            secret
        );
        assert_eq!(
            public_key_from_bytes("public", &public_key_bytes(&public)).unwrap(),
            public
        );
        assert!(public_key_from_bytes("public", &[0xff; 32]).is_err());
    }

    /// The challenge this file computes is the one a signature built the device's way verifies against.
    ///
    /// This is the closed loop that makes [`script_signature_challenge`] worth trusting **without a device**: sign
    /// exactly as `get_script_signature` does - same argument order, same ephemeral commitment - against a
    /// challenge from this function, and verify it against the same challenge. If the borsh length prefixes or the
    /// network label were wrong, `sign` and `verify` would still agree with each other and this would pass; what it
    /// pins is the *argument order and the statement*, which is the half that a simulator run cannot isolate.
    #[test]
    fn a_signature_built_the_devices_way_verifies_against_this_challenge() {
        let factory = commitment_factory();
        let value = random_secret_key();
        let commitment_private_key = random_secret_key();
        let script_private_key = random_secret_key();
        let script_public_key = RistrettoPublicKey::from_secret_key(&script_private_key);
        let commitment = script_commitment(&commitment_private_key, &value);
        let message = random_bytes_32();

        let r_a = random_secret_key();
        let r_x = random_secret_key();
        let r_y = random_secret_key();
        let ephemeral_commitment = factory.commit(&r_x, &r_a);
        let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&r_y);

        let challenge = script_signature_challenge(
            0,
            &ephemeral_commitment,
            &ephemeral_pubkey,
            &script_public_key,
            &commitment,
            &message,
        );

        // Spelled out rather than inferred. The concrete type is reachable from `commitment` below, but resolving
        // it through `sign`'s `for<'b> &'b K: ...` bounds overflows the trait solver under `cargo clippy` and the
        // overflow arrives as an `E0275` about `Simd`, which points at nothing.
        let signature: RistrettoComAndPubSig = RistrettoComAndPubSig::sign(
            &value,
            &commitment_private_key,
            &script_private_key,
            &r_a,
            &r_x,
            &r_y,
            &challenge,
            &factory,
        )
        .expect("the challenge must be signable");

        assert!(
            signature.verify_challenge(&commitment, &script_public_key, &challenge, &factory, &mut rand::rng()),
            "a signature built exactly as the device builds it did not verify"
        );
    }

    /// The challenge depends on every one of its inputs. A hasher that dropped a field - the commitment, say -
    /// would produce a challenge a substituted commitment still verified against, which is precisely the forgery
    /// the domain separated hash is there to prevent.
    #[test]
    fn every_input_changes_the_challenge() {
        let factory = commitment_factory();
        let a = random_secret_key();
        let b = random_secret_key();
        let commitment_a = factory.commit(&a, &b);
        let commitment_b = factory.commit(&b, &a);
        let key_a = RistrettoPublicKey::from_secret_key(&a);
        let key_b = RistrettoPublicKey::from_secret_key(&b);
        let message_a = random_bytes_32();
        let message_b = random_bytes_32();

        let base = script_signature_challenge(0, &commitment_a, &key_a, &key_a, &commitment_a, &message_a);
        let variants = [
            script_signature_challenge(1, &commitment_a, &key_a, &key_a, &commitment_a, &message_a),
            script_signature_challenge(0, &commitment_b, &key_a, &key_a, &commitment_a, &message_a),
            script_signature_challenge(0, &commitment_a, &key_b, &key_a, &commitment_a, &message_a),
            script_signature_challenge(0, &commitment_a, &key_a, &key_b, &commitment_a, &message_a),
            script_signature_challenge(0, &commitment_a, &key_a, &key_a, &commitment_b, &message_a),
            script_signature_challenge(0, &commitment_a, &key_a, &key_a, &commitment_a, &message_b),
        ];
        for (index, variant) in variants.iter().enumerate() {
            assert_ne!(
                base.to_vec(),
                variant.to_vec(),
                "changing input {index} did not change the challenge"
            );
        }
    }

    /// `alpha_hasher` is `H(b) + alpha`, so its public key is `H(b) * G + alpha * G`. This is the identity the
    /// script offset and script signature scenarios check a derived script key with, and it is the only reason
    /// those scenarios can verify a key the device derived from the wallet's spend key without ever seeing it.
    #[test]
    fn the_derived_script_public_key_is_the_hash_plus_alpha_times_g() {
        let alpha = random_secret_key();
        let public_spend_key = RistrettoPublicKey::from_secret_key(&alpha);
        let blinding_factor = random_secret_key();

        // What the device computes, in secret key form.
        let on_device = alpha_derived_script_key(&blinding_factor) + alpha;

        assert_eq!(
            RistrettoPublicKey::from_secret_key(&on_device),
            alpha_derived_script_public_key(&blinding_factor, &public_spend_key)
        );
    }

    /// Two different blinding factors must give two different script keys, or the derived script key path would be
    /// folding the same key every time and the script offset scenarios would pass vacuously.
    #[test]
    fn different_blinding_factors_derive_different_script_keys() {
        assert_ne!(
            alpha_derived_script_key(&random_secret_key()),
            alpha_derived_script_key(&random_secret_key())
        );
    }

    /// The metadata signature challenge is a different hash from the script signature challenge, even given
    /// identical inputs.
    ///
    /// They share an encoding and differ in their label and in their field order, so the plausible mistake is to
    /// build one from the other and change only the label. That would produce a challenge which hashes perfectly
    /// well and verifies against nothing, and the failure would look like a broken device.
    #[test]
    fn the_two_challenges_are_not_the_same_hash() {
        let factory = commitment_factory();
        let a = random_secret_key();
        let b = random_secret_key();
        let commitment = factory.commit(&a, &b);
        let key = RistrettoPublicKey::from_secret_key(&a);
        let message = random_bytes_32();

        assert_ne!(
            script_signature_challenge(0, &commitment, &key, &key, &commitment, &message).to_vec(),
            metadata_signature_challenge(0, &key, &commitment, &key, &commitment, &message).to_vec()
        );
    }

    /// A signature built the way `handler_get_one_sided_metadata_signature` builds one verifies against the
    /// challenge this file computes. Same closed loop, and the same argument, as the script signature test above.
    #[test]
    fn a_metadata_signature_built_the_devices_way_verifies_against_this_challenge() {
        let factory = commitment_factory();
        let value = RistrettoSecretKey::from(12_345u64);
        let commitment_mask = random_secret_key();
        let sender_offset_private_key = random_secret_key();
        let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_private_key);
        let receiver_spend_key = RistrettoPublicKey::from_secret_key(&random_secret_key());

        let commitment = factory.commit(&commitment_mask, &value);
        let r_a = random_secret_key();
        let r_x = random_secret_key();
        let r_y = random_secret_key();
        let ephemeral_commitment = factory.commit(&r_x, &r_a);
        let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&r_y);

        let script = stealth_script(&commitment_mask, &receiver_spend_key);
        let message = metadata_signature_message(0, &script, &random_bytes_32());
        let challenge = metadata_signature_challenge(
            0,
            &sender_offset_public_key,
            &ephemeral_commitment,
            &ephemeral_pubkey,
            &commitment,
            &message,
        );

        let signature: RistrettoComAndPubSig = RistrettoComAndPubSig::sign(
            &value,
            &commitment_mask,
            &sender_offset_private_key,
            &r_a,
            &r_x,
            &r_y,
            &challenge,
            &factory,
        )
        .expect("the challenge must be signable");

        assert!(
            signature.verify_challenge(
                &commitment,
                &sender_offset_public_key,
                &challenge,
                &factory,
                &mut rand::rng()
            ),
            "a metadata signature built exactly as the device builds it did not verify"
        );
    }

    /// The stealth script is the 34 byte `length | opcode | key` the device hashes, and it moves with both of its
    /// inputs. A script that ignored the commitment mask would make every one sided output to a given receiver
    /// share a script, which is the whole thing stealth addresses exist to avoid.
    #[test]
    fn the_stealth_script_is_a_push_pubkey_of_the_stealth_key() {
        let mask = random_secret_key();
        let receiver = RistrettoPublicKey::from_secret_key(&random_secret_key());
        let script = stealth_script(&mask, &receiver);

        assert_eq!(script.len(), 34);
        assert_eq!(script.first(), Some(&33));
        assert_eq!(script.get(1), Some(&0x7e));
        assert_eq!(
            script.get(2..),
            Some((receiver.clone() + RistrettoPublicKey::from_secret_key(&alpha_derived_script_key(&mask))).as_bytes())
        );

        assert_ne!(script, stealth_script(&random_secret_key(), &receiver));
        assert_ne!(
            script,
            stealth_script(&mask, &RistrettoPublicKey::from_secret_key(&random_secret_key()))
        );
    }

    /// The published receiver parses, is a dual address, and grows a payment ID without changing its keys.
    ///
    /// The second half is what makes the two review scenarios comparable: if adding a payment ID also moved the
    /// spend key, the scenario that asserts a `Payment ID` row would differ from the one that asserts its absence
    /// in two ways rather than one.
    #[test]
    fn the_published_receiver_parses_and_keeps_its_keys() {
        let plain = published_receiver(0).expect("the published address must parse");
        let with_payment_id = published_receiver(32).expect("32 bytes is well within the limit");

        assert_eq!(plain.public_spend_key(), with_payment_id.public_spend_key());
        assert_eq!(plain.public_view_key(), with_payment_id.public_view_key());
        assert_ne!(plain.to_base58(), with_payment_id.to_base58());
        assert_eq!(plain.to_base58(), RECEIVER_BASE58);
        assert!(plain.public_spend_key().to_public_key().is_ok());
    }

    /// The commitment really is a Pedersen commitment under the device's generators, and swapping its two
    /// arguments is a visible change rather than a no-op - which is what makes the argument order comment above
    /// something a reader can check.
    #[test]
    fn the_script_commitment_is_not_symmetric_in_its_arguments() {
        let k = random_secret_key();
        let v = random_secret_key();
        assert_ne!(script_commitment(&k, &v), script_commitment(&v, &k));
        assert!(commitment_factory().open(&k, &v, &script_commitment(&k, &v)));
    }
}
