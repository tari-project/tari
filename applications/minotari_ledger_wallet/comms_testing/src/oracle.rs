// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! A host side reimplementation of the device's key derivation, used as an **independent oracle**.
//!
//! # Why this exists
//!
//! A table of frozen vectors only ever says "the device still does what it did the day we captured this". It cannot
//! say whether what it did was *right*. This module derives the same keys from the same seed using published
//! specifications only - BIP-39, BIP-32, and the Tari domain separated hashing already in `tari_crypto` - so that
//! the vector table can be checked against a construction nobody read off the device. If the oracle and the device
//! disagree, one of them is wrong and the build fails; that is a strictly stronger statement than a frozen table.
//!
//! # Why an oracle is possible at all (the Spec 2 "Step 0" question)
//!
//! `wallet/src/utils.rs::get_raw_bip32_key` hands `bip32_derive` a 64 byte buffer:
//!
//! ```text
//! let mut key_buffer = Zeroizing::new([0u8; 64]);
//! bip32_derive(CurvesId::Secp256k1, path, key_buffer.as_mut(), None)
//! ```
//!
//! The obvious reading of a 64 byte BIP32 output is "private key followed by chain code". **That reading is wrong**,
//! and the difference matters because the whole buffer is then hashed. What actually happens is:
//!
//! * `ledger_device_sdk::ecc::bip32_derive` takes the chain code as a **separate** `cc: Option<&mut [u8]>`
//!   out-parameter, which `get_raw_bip32_key` passes as `None`. The chain code is therefore never written into
//!   `key_buffer` at all. The SDK's own `impl SeedDerive for Secp256k1` confirms the split: it asks for the chain code
//!   in `cc` and takes only `tmp[..32]` as the key.
//! * The syscall behind it writes **32 bytes** for a Weierstrass curve. Read directly out of the Speculos
//!   implementation, `src/bolos/os_bip32.c::hdw_bip32`, whose tail is `if (private_key != NULL) memcpy(private_key,
//!   key->private_key, 32);`
//!
//!   That is a statement about Speculos, which is what this fixture runs. BOLOS itself is closed, so the same claim
//!   about real hardware rests on the syscall ABI being shared - Speculos emulates it - and on the SDK's own
//!   `SeedDerive for Secp256k1` taking the chain code from `cc` rather than from the tail of the key buffer. That is
//!   strong, but it is inference, not a reading. Nothing here has been run against a physical device.
//! * The SDK nonetheless *requires* `key.len() >= 64` for every supported curve. That is a maximum across curves, not a
//!   statement about secp256k1: the Ed25519-BIP32 path (`hdw_bip32_ed25519`) really does write 64 bytes, because its
//!   key is the `kL || kR` pair.
//!
//! So bytes 32..64 of `key_buffer` are **never written**. They are the zeros the buffer was initialised with, and
//! they are hashed as zeros. The hash preimage is `bip32_secp256k1_private_key || [0u8; 32]`.
//!
//! Everything upstream of that is textbook: `expand_seed` is BIP-32 master key generation with the HMAC-SHA512 key
//! `"Bitcoin seed"`, and `hdw_bip32` is BIP-32 CKDpriv including the "IL >= n or child == 0, retry with 0x01 || IR"
//! rule. Given the seed, the derivation is reproducible from published specifications. The oracle is buildable.
//!
//! # The fragility this pins down
//!
//! Because the second half of the preimage is uninitialised-by-contract rather than chosen, every key this wallet
//! owns depends on the SDK continuing not to write there. A future SDK or BOLOS release that decided to fill the
//! remaining 32 bytes with, say, the chain code would silently change **every derived key on every account** - a
//! total, unrecoverable loss of funds for existing users, with no compile error anywhere. The vector table is the
//! thing that turns that into a failing test.

use blake2::Blake2b;
use digest::consts::U64;
use hmac::{Hmac, Mac};
use k256::{
    Scalar,
    elliptic_curve::{PrimeField, ops::MulByGenerator, sec1::ToEncodedPoint},
};
use sha2::{Sha512, digest::FixedOutput};
use tari_crypto::{
    hash_domain,
    hashing::DomainSeparatedHasher,
    keys::{PublicKey, SecretKey},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use zeroize::Zeroizing;

/// The BIP32 coin type Tari registered, from `wallet/src/main.rs`.
pub const BIP32_COIN_TYPE: u32 = 535348;

/// The device's fixed index for the account spend key, from `wallet/src/main.rs::STATIC_SPEND_INDEX`.
pub const STATIC_SPEND_INDEX: u64 = 42;

/// The device's fixed index for the account view key, from `wallet/src/main.rs::STATIC_VIEW_INDEX`.
pub const STATIC_VIEW_INDEX: u64 = 57311;

/// The device's `KeyType` discriminants, from `wallet/src/main.rs`.
///
/// These are the **last path element**, so they are part of the derivation and not just a dispatch tag. They are
/// mirrored here rather than imported because `minotari_ledger_wallet_common` exposes `LedgerKeyBranch` - the value
/// the *host* sends - and the device maps that onto a different set of numbers before building the path. Getting
/// the two confused would produce an oracle that agrees with itself and with nothing else.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KeyType {
    Spend = 0x01,
    ViewKey = 0x03,
    OneSidedSenderOffset = 0x04,
    Random = 0x06,
    PreMine = 0x07,
}

impl KeyType {
    /// The discriminant as the device writes it into the derivation path.
    pub const fn as_byte(self) -> u8 {
        self as u8
    }
}

hash_domain!(LedgerHashDomain, "com.tari.minotari_ledger_wallet", 0);

/// The label `get_raw_key_hash` uses, from `wallet/src/utils.rs`.
const RAW_KEY_LABEL: &str = "raw_key";

/// The HMAC-SHA512 key BIP-32 specifies for secp256k1 master key generation.
const BIP32_SECP_SEED: &[u8] = b"Bitcoin seed";

/// BIP-39's PBKDF2 iteration count.
const BIP39_PBKDF2_ROUNDS: u32 = 2048;

type HmacSha512 = Hmac<Sha512>;

/// A BIP-32 extended private key: the 32 byte key and its 32 byte chain code.
#[derive(Clone)]
struct ExtendedPrivateKey {
    private_key: Zeroizing<[u8; 32]>,
    chain_code: Zeroizing<[u8; 32]>,
}

/// Errors the oracle can produce.
///
/// Every one of these is "the inputs were not what BIP-32 or BIP-39 allows". None of them is reachable from a
/// well-formed vector, which is why the table itself carries no error handling.
#[derive(Debug, PartialEq, Eq)]
pub enum OracleError {
    /// A derivation step could not produce a valid child key. BIP-32 makes this astronomically unlikely, and it is
    /// only representable at all because the specification says what to do if it happens.
    InvalidChildKey(u32),
    /// The seed was not a length BIP-32 accepts.
    InvalidSeedLength(usize),
    /// The 64 uniform bytes could not be reduced to a Ristretto scalar. `from_uniform_bytes` is total over 64 byte
    /// inputs, so this only fires if the slice length is wrong.
    UniformBytes(String),
}

impl core::fmt::Display for OracleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            OracleError::InvalidChildKey(index) => {
                write!(f, "BIP-32 could not derive a valid child key at index {index}")
            },
            OracleError::InvalidSeedLength(length) => {
                write!(f, "A BIP-32 seed must be 16 to 64 bytes, got {length}")
            },
            OracleError::UniformBytes(e) => write!(f, "Could not reduce the hash to a secret key: {e}"),
        }
    }
}

impl std::error::Error for OracleError {}

/// Turn a BIP-39 mnemonic into the 64 byte seed a Ledger device is loaded with.
///
/// BIP-39 defines this as PBKDF2-HMAC-SHA512 over the NFKD-normalised mnemonic, with the salt `"mnemonic"` followed
/// by the passphrase, for 2048 rounds. PBKDF2 is spelled out here rather than pulled in as a dependency: it is
/// eleven lines, and an oracle whose job is to be checkable is better off with no opaque third party step in it.
///
/// Normalisation is not performed. Every mnemonic this crate uses is plain ASCII, where NFKD is the identity; a
/// caller that supplies a non-ASCII mnemonic would need to normalise it first.
pub fn bip39_seed(mnemonic: &str, passphrase: &str) -> Zeroizing<[u8; 64]> {
    let mut salt = Vec::with_capacity(8usize.saturating_add(passphrase.len()).saturating_add(4));
    salt.extend_from_slice(b"mnemonic");
    salt.extend_from_slice(passphrase.as_bytes());
    // PBKDF2 block index 1, big endian. One block of SHA-512 is 64 bytes, which is the whole output.
    salt.extend_from_slice(&1u32.to_be_bytes());

    let mut previous = hmac_sha512(mnemonic.as_bytes(), &salt);
    let mut seed = previous;
    for _ in 1..BIP39_PBKDF2_ROUNDS {
        previous = hmac_sha512(mnemonic.as_bytes(), &previous);
        for (accumulated, block) in seed.iter_mut().zip(previous.iter()) {
            *accumulated ^= *block;
        }
    }
    Zeroizing::new(seed)
}

fn hmac_sha512(key: &[u8], data: &[u8]) -> [u8; 64] {
    // `new_from_slice` is infallible for HMAC - the construction accepts keys of every length, short ones padded
    // and long ones hashed down - so `InvalidLength` is not reachable.
    //
    // It is still spelled as a panic rather than a fallback. An earlier version quietly retried with an empty key,
    // which is the worse failure by a distance: it would have produced a perfectly well-formed 64 byte output
    // derived from the *wrong* key, and every vector downstream of it would have been confidently, silently wrong.
    // In an oracle whose only job is to be right, a loud stop beats a plausible answer.
    let mut mac = match <HmacSha512 as Mac>::new_from_slice(key) {
        Ok(mac) => mac,
        Err(e) => panic!(
            "HMAC-SHA512 rejected a {} byte key, which cannot happen: {e}",
            key.len()
        ),
    };
    mac.update(data);
    let mut out = [0u8; 64];
    out.copy_from_slice(&mac.finalize_fixed());
    out
}

/// BIP-32 master key generation: `I = HMAC-SHA512(Key = "Bitcoin seed", Data = seed)`.
fn master_key(seed: &[u8]) -> Result<ExtendedPrivateKey, OracleError> {
    if seed.len() < 16 || seed.len() > 64 {
        return Err(OracleError::InvalidSeedLength(seed.len()));
    }
    let i = Zeroizing::new(hmac_sha512(BIP32_SECP_SEED, seed));
    let mut key = ExtendedPrivateKey {
        private_key: Zeroizing::new([0u8; 32]),
        chain_code: Zeroizing::new([0u8; 32]),
    };
    key.private_key.copy_from_slice(i.get(..32).unwrap_or_default());
    key.chain_code.copy_from_slice(i.get(32..).unwrap_or_default());

    // BIP-32 says an out-of-range master key is invalid. Speculos' `expand_seed` instead re-hashes until it lands
    // in range, so this loop mirrors Speculos rather than the letter of BIP-32 - for any seed either party would
    // accept, the two agree, and the difference is only reachable with probability ~2^-127.
    while !is_valid_scalar(&key.private_key) {
        let mut rehash = Zeroizing::new([0u8; 64]);
        rehash
            .get_mut(..32)
            .unwrap_or_default()
            .copy_from_slice(key.private_key.as_ref());
        rehash
            .get_mut(32..)
            .unwrap_or_default()
            .copy_from_slice(key.chain_code.as_ref());
        let i = Zeroizing::new(hmac_sha512(BIP32_SECP_SEED, rehash.as_ref()));
        key.private_key.copy_from_slice(i.get(..32).unwrap_or_default());
        key.chain_code.copy_from_slice(i.get(32..).unwrap_or_default());
    }
    Ok(key)
}

/// Whether 32 big endian bytes are a valid secp256k1 private key: non-zero and less than the group order.
fn is_valid_scalar(bytes: &[u8; 32]) -> bool {
    scalar_from_bytes(bytes).map(|s| s != Scalar::ZERO).unwrap_or(false)
}

/// Interpret 32 big endian bytes as a secp256k1 scalar, rejecting anything at or above the group order.
///
/// `Scalar::from_repr` is the check BIP-32 calls "parse256(IL) >= n", done in constant time.
fn scalar_from_bytes(bytes: &[u8; 32]) -> Option<Scalar> {
    Option::from(Scalar::from_repr((*bytes).into()))
}

/// BIP-32 CKDpriv: derive the child at `index` from `parent`.
fn derive_child(parent: &ExtendedPrivateKey, index: u32) -> Result<ExtendedPrivateKey, OracleError> {
    let parent_scalar = scalar_from_bytes(&parent.private_key).ok_or(OracleError::InvalidChildKey(index))?;

    // The 37 byte HMAC input: 33 bytes of key material then the index, big endian.
    let mut data = [0u8; 37];
    if index & 0x8000_0000 == 0 {
        // Non-hardened: the parent's *public* key, compressed SEC1. This is the step that makes a real secp256k1
        // implementation necessary - a hardened-only path would need no curve arithmetic at all.
        let point = k256::ProjectivePoint::mul_by_generator(&parent_scalar).to_affine();
        let encoded = point.to_encoded_point(true);
        data.get_mut(..33)
            .unwrap_or_default()
            .copy_from_slice(encoded.as_bytes());
    } else {
        // Hardened: a zero byte then the parent's private key.
        data.get_mut(1..33)
            .unwrap_or_default()
            .copy_from_slice(parent.private_key.as_ref());
    }
    data.get_mut(33..)
        .unwrap_or_default()
        .copy_from_slice(&index.to_be_bytes());

    let mut attempt = Zeroizing::new(hmac_sha512(parent.chain_code.as_ref(), &data));
    // BIP-32's retry rule. Unreachable in practice; present because the specification says what to do.
    for _ in 0..8 {
        let mut left = [0u8; 32];
        left.copy_from_slice(attempt.get(..32).unwrap_or_default());
        if let Some(left_scalar) = scalar_from_bytes(&left) {
            let child_scalar = left_scalar + parent_scalar;
            if child_scalar != Scalar::ZERO {
                let mut child = ExtendedPrivateKey {
                    private_key: Zeroizing::new([0u8; 32]),
                    chain_code: Zeroizing::new([0u8; 32]),
                };
                // `to_repr` is `PrimeField`'s canonical encoding, which for secp256k1 is 32 big endian bytes -
                // the same `ser256` BIP-32 specifies.
                child.private_key.copy_from_slice(&child_scalar.to_repr());
                child.chain_code.copy_from_slice(attempt.get(32..).unwrap_or_default());
                return Ok(child);
            }
        }
        // Retry with `0x01 || IR || ser32(index)`.
        data.get_mut(..1).unwrap_or_default().copy_from_slice(&[0x01]);
        data.get_mut(1..33)
            .unwrap_or_default()
            .copy_from_slice(attempt.get(32..).unwrap_or_default());
        attempt = Zeroizing::new(hmac_sha512(parent.chain_code.as_ref(), &data));
    }
    Err(OracleError::InvalidChildKey(index))
}

/// The 32 byte BIP-32 secp256k1 private key at `path`, which is what the device's `bip32_derive` writes into the
/// first half of its buffer.
pub fn bip32_secp256k1_private_key(seed: &[u8], path: &[u32]) -> Result<Zeroizing<[u8; 32]>, OracleError> {
    let mut key = master_key(seed)?;
    for index in path {
        key = derive_child(&key, *index)?;
    }
    Ok(key.private_key)
}

/// Build the derivation path exactly as `wallet/src/utils.rs::derive_from_bip32_key` does.
///
/// The device formats `m/44'/535348'/{account}'/0/{index}'/{key_type}` into a string and hands it to the SDK's
/// `make_bip32_path`, which parses it back out. Two consequences are reproduced here rather than tidied up, because
/// the device is the specification and the point of the oracle is to agree with it:
///
/// * Only elements written with a `'` are hardened. `0` and the trailing key type are **not**, which is why the oracle
///   needs secp256k1 point arithmetic at all.
/// * `make_bip32_path` accumulates each element into a `u32` with `acc * 10 + digit`, then adds `0x80000000` for
///   hardening. The device application is a release build, so both of those wrap rather than panic. A `u64` account -
///   and the host does send random `u64` accounts - therefore addresses the path element congruent to it modulo 2^32,
///   not a distinct one. `wrapping_*` here is deliberate and matches the device; see the
///   `account_beyond_u32_wraps_onto_the_low_word` vector.
pub fn derivation_path(account: u64, index: u64, key_type: KeyType) -> [u32; 6] {
    [
        0x8000_0000u32.wrapping_add(44),
        0x8000_0000u32.wrapping_add(BIP32_COIN_TYPE),
        0x8000_0000u32.wrapping_add(decimal_to_wrapping_u32(account)),
        0,
        0x8000_0000u32.wrapping_add(decimal_to_wrapping_u32(index)),
        u32::from(key_type.as_byte()),
    ]
}

/// Re-parse a `u64` the way `make_bip32_path` parses its decimal rendering: digit by digit into a wrapping `u32`.
fn decimal_to_wrapping_u32(value: u64) -> u32 {
    let mut accumulator = 0u32;
    for digit in value.to_string().bytes() {
        accumulator = accumulator
            .wrapping_mul(10)
            .wrapping_add(u32::from(digit.saturating_sub(b'0')));
    }
    accumulator
}

/// The device's `get_raw_key_hash`: hash the 64 byte BIP32 buffer under the ledger domain, label `"raw_key"`.
///
/// The buffer is `bip32_private_key || [0u8; 32]` - see the module docs for why the tail is zeros rather than the
/// chain code. `DomainSeparatedHasher::chain` length-prefixes its input, and `wallet/src/crypto/hashing.rs` is a
/// byte-for-byte copy of the `tari_crypto` implementation used here, so the two preimages are identical.
fn raw_key_hash(bip32_private_key: &[u8; 32]) -> Zeroizing<[u8; 64]> {
    let mut buffer = Zeroizing::new([0u8; 64]);
    buffer
        .get_mut(..32)
        .unwrap_or_default()
        .copy_from_slice(bip32_private_key);

    let hash = DomainSeparatedHasher::<Blake2b<U64>, LedgerHashDomain>::new_with_label(RAW_KEY_LABEL)
        .chain(buffer.as_ref())
        .finalize();

    let mut hashed = Zeroizing::new([0u8; 64]);
    hashed.copy_from_slice(hash.as_ref());
    hashed
}

/// Derive the secret key the device would derive for `(account, index, key_type)` from `seed`.
///
/// This is the whole of `derive_from_bip32_key`, host side: BIP-32 to a secp256k1 private key, domain separated
/// Blake2b-512 over that key padded to 64 bytes, then wide reduction onto a Ristretto scalar.
pub fn derive_secret_key(
    seed: &[u8],
    account: u64,
    index: u64,
    key_type: KeyType,
) -> Result<RistrettoSecretKey, OracleError> {
    let path = derivation_path(account, index, key_type);
    let bip32_key = bip32_secp256k1_private_key(seed, &path)?;
    let uniform = raw_key_hash(&bip32_key);
    RistrettoSecretKey::from_uniform_bytes(uniform.as_ref()).map_err(|e| OracleError::UniformBytes(e.to_string()))
}

/// Derive the public key the device would return for `(account, index, key_type)`.
pub fn derive_public_key(
    seed: &[u8],
    account: u64,
    index: u64,
    key_type: KeyType,
) -> Result<RistrettoPublicKey, OracleError> {
    Ok(RistrettoPublicKey::from_secret_key(&derive_secret_key(
        seed, account, index, key_type,
    )?))
}

/// The account spend key the device returns for `GetPublicSpendKey`.
pub fn derive_public_spend_key(seed: &[u8], account: u64) -> Result<RistrettoPublicKey, OracleError> {
    derive_public_key(seed, account, STATIC_SPEND_INDEX, KeyType::Spend)
}

/// The account view key the device returns for `GetViewKey`. Note that this instruction returns the **secret** key.
pub fn derive_view_key(seed: &[u8], account: u64) -> Result<RistrettoSecretKey, OracleError> {
    derive_secret_key(seed, account, STATIC_VIEW_INDEX, KeyType::ViewKey)
}

#[cfg(test)]
mod test {
    use tari_utilities::hex::Hex;

    use super::*;
    use crate::seeds::{ALTERNATE_SEED_MNEMONIC, SPECULOS_DEFAULT_SEED_BYTES, SPECULOS_DEFAULT_SEED_MNEMONIC};

    /// BIP-39 is checked against Speculos' own copy of the seed for its default mnemonic, which Speculos carries as
    /// a raw byte array in `src/environment.c`. Agreeing with it means the oracle and the simulator start from the
    /// same 64 bytes - the one assumption the rest of the oracle rests on.
    #[test]
    fn bip39_reproduces_the_speculos_default_seed() {
        let seed = bip39_seed(SPECULOS_DEFAULT_SEED_MNEMONIC, "");
        assert_eq!(seed.as_ref().to_vec().to_hex(), SPECULOS_DEFAULT_SEED_BYTES.to_hex());
    }

    /// BIP-32 test vector 1 from the BIP itself, via the chain of extended private keys. Checking the oracle's
    /// derivation against the specification's own numbers is what makes it an oracle rather than a second opinion.
    ///
    /// Seed `000102030405060708090a0b0c0d0e0f`, path `m/0'/1/2'/2/1000000000`. The expected private keys are the
    /// `xprv` payloads published in BIP-32.
    #[test]
    fn bip32_matches_the_published_test_vector_1() {
        let seed = <[u8; 16]>::from_hex("000102030405060708090a0b0c0d0e0f").unwrap();

        let cases: [(&[u32], &str); 6] = [
            (&[], "e8f32e723decf4051aefac8e2c93c9c5b214313817cdb01a1494b917c8436b35"),
            (
                &[0x8000_0000],
                "edb2e14f9ee77d26dd93b4ecede8d16ed408ce149b6cd80b0715a2d911a0afea",
            ),
            (
                &[0x8000_0000, 1],
                "3c6cb8d0f6a264c91ea8b5030fadaa8e538b020f0a387421a12de9319dc93368",
            ),
            (
                &[0x8000_0000, 1, 0x8000_0002],
                "cbce0d719ecf7431d88e6a89fa1483e02e35092af60c042b1df2ff59fa424dca",
            ),
            (
                &[0x8000_0000, 1, 0x8000_0002, 2],
                "0f479245fb19a38a1954c5c7c0ebab2f9bdfd96a17563ef28a6a4b1a2a764ef4",
            ),
            (
                &[0x8000_0000, 1, 0x8000_0002, 2, 1_000_000_000],
                "471b76e389e528d6de6d816857e012c5455051cad6660850e58372a6c3e6e7c8",
            ),
        ];

        for (path, expected) in cases {
            let key = bip32_secp256k1_private_key(&seed, path).unwrap();
            assert_eq!(key.as_ref().to_vec().to_hex(), expected, "BIP-32 path {path:?}");
        }
    }

    /// The path construction is the easiest thing in here to get subtly wrong, so it is asserted element by element
    /// rather than only through the keys it produces.
    #[test]
    fn the_derivation_path_matches_the_device() {
        // m/44'/535348'/0'/0/0'/4
        assert_eq!(derivation_path(0, 0, KeyType::OneSidedSenderOffset), [
            0x8000_002C,
            0x8000_0000 + 535_348,
            0x8000_0000,
            0,
            0x8000_0000,
            4,
        ]);
        // The `0` element is not hardened, and the key type is not either.
        let path = derivation_path(7, 9, KeyType::ViewKey);
        assert_eq!(path[3], 0);
        assert_eq!(path[5], 3);
        assert_eq!(path[2], 0x8000_0000 + 7);
        assert_eq!(path[4], 0x8000_0000 + 9);
    }

    /// `make_bip32_path` parses the decimal rendering into a wrapping `u32`, so accounts 2^32 apart collide.
    #[test]
    fn a_u64_account_wraps_onto_a_u32_path_element() {
        assert_eq!(decimal_to_wrapping_u32(0), 0);
        assert_eq!(decimal_to_wrapping_u32(u64::from(u32::MAX)), u32::MAX);
        assert_eq!(decimal_to_wrapping_u32(1u64 << 32), 0);
        assert_eq!(
            derivation_path(1u64 << 32, 0, KeyType::Random),
            derivation_path(0, 0, KeyType::Random)
        );
    }

    /// Two different seeds must not produce the same key. Trivially true, and cheap insurance against an oracle
    /// that ignores its seed argument - which is exactly the failure mode that would make it agree with a device
    /// that returns a constant.
    #[test]
    fn the_oracle_depends_on_the_seed() {
        let alternate = bip39_seed(ALTERNATE_SEED_MNEMONIC, "");
        let a = derive_public_key(SPECULOS_DEFAULT_SEED_BYTES, 0, 0, KeyType::OneSidedSenderOffset).unwrap();
        let b = derive_public_key(alternate.as_ref(), 0, 0, KeyType::OneSidedSenderOffset).unwrap();
        assert_ne!(a, b);
    }

    /// The zero tail is the whole Step 0 finding, so it gets an assertion of its own: hashing
    /// `private_key || zeros` must not be the same as hashing `private_key || chain_code`, and the oracle must be
    /// doing the former.
    #[test]
    fn the_hash_preimage_is_padded_with_zeros_not_the_chain_code() {
        let key = [7u8; 32];
        let with_zeros = raw_key_hash(&key);

        let mut with_chain_code = [0u8; 64];
        with_chain_code[..32].copy_from_slice(&key);
        with_chain_code[32..].copy_from_slice(&[9u8; 32]);
        let other = DomainSeparatedHasher::<Blake2b<U64>, LedgerHashDomain>::new_with_label(RAW_KEY_LABEL)
            .chain(with_chain_code)
            .finalize();

        assert_ne!(with_zeros.as_ref(), other.as_ref());

        let mut padded = [0u8; 64];
        padded[..32].copy_from_slice(&key);
        let expected = DomainSeparatedHasher::<Blake2b<U64>, LedgerHashDomain>::new_with_label(RAW_KEY_LABEL)
            .chain(padded)
            .finalize();
        assert_eq!(with_zeros.as_ref(), expected.as_ref());
    }
}
