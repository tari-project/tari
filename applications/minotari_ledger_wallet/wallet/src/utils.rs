// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;
use core::ops::Deref;

use blake2::Blake2b;
use digest::{consts::U64, Digest};
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::NbglStatus;
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::gadgets::{MessageScroller, SingleMessage};
use ledger_device_sdk::{
    ecc::{bip32_derive, make_bip32_path, CurvesId, CxError},
    random::LedgerRng,
};
use minotari_ledger_wallet_common::u64_to_string;
use rand_core::RngCore;
use tari_utilities::ByteArray;
use zeroize::Zeroizing;

use crate::{
    alloc::string::{String, ToString},
    crypto::{hashing::DomainSeparatedHasher, keys::RistrettoSecretKey},
    hash_domain,
    AppSW,
    KeyType,
    BIP32_COIN_TYPE,
};

hash_domain!(LedgerHashDomain, "com.tari.minotari_ledger_wallet", 0);
hash_domain!(
    KeyManagerTransactionsHashDomain,
    "com.tari.base_layer.core.transactions.key_manager",
    1
);

// Convert CxError to a string for display
fn cx_error_to_string(e: CxError) -> String {
    let err = match e {
        CxError::Carry => "Carry",
        CxError::Locked => "Locked",
        CxError::Unlocked => "Unlocked",
        CxError::NotLocked => "NotLocked",
        CxError::NotUnlocked => "NotUnlocked",
        CxError::InternalError => "InternalError",
        CxError::InvalidParameterSize => "InvalidParameterSize",
        CxError::InvalidParameterValue => "InvalidParameterValue",
        CxError::InvalidParameter => "InvalidParameter",
        CxError::NotInvertible => "NotInvertible",
        CxError::Overflow => "Overflow",
        CxError::MemoryFull => "MemoryFull",
        CxError::NoResidue => "NoResidue",
        CxError::PointAtInfinity => "PointAtInfinity",
        CxError::InvalidPoint => "InvalidPoint",
        CxError::InvalidCurve => "InvalidCurve",
        CxError::GenericError => "GenericError",
    };
    err.to_string()
}

// Get a raw 64 byte key hash from the BIP32 path.
// Note: We use `CurvesId::Secp256k1` as the curve for the bip32 key derivation because it provides better entropy when
//       compared to `CurvesId::Ed25519`. There is also no need for compatibility to `tari_crypto` as the output is only
//       ever used in a subsequent key derivation function.
fn get_raw_bip32_key(path: &[u32]) -> Result<Zeroizing<[u8; 64]>, String> {
    let mut key_buffer = Zeroizing::new([0u8; 64]);
    match bip32_derive(CurvesId::Secp256k1, path, key_buffer.as_mut(), None) {
        Ok(_) => {
            if key_buffer.deref() == &[0u8; 64] {
                return Err(cx_error_to_string(CxError::InternalError));
            } else {
                Ok(key_buffer)
            }
        },
        Err(e) => return Err(cx_error_to_string(e)),
    }
}

//  This function applies domain separated hashing to the 64 byte private key of the returned buffer to get 64
//  uniformly distributed random bytes.
fn get_raw_key_hash(path: &[u32]) -> Result<Zeroizing<[u8; 64]>, String> {
    let raw_key_64 = get_raw_bip32_key(path)?;

    let mut raw_key_hashed = Zeroizing::new([0u8; 64]);
    DomainSeparatedHasher::<Blake2b<U64>, LedgerHashDomain>::new_with_label("raw_key")
        .chain(&raw_key_64.as_ref())
        .finalize_into(raw_key_hashed.as_mut().into());
    Ok(raw_key_hashed)
}

/// Derive a secret key from a BIP32 path. In case of an error, display an interactive message on the device.
///
/// The path is `m/44'/{coin}'/{account}'/{index >> 32}/{index & 0xFFFF_FFFF}'/{key_type}`: every bit of the `u64`
/// index is derived from. A BIP32 path element is a `u32`, and the SDK's `make_bip32_path` accumulates each one in a
/// wrapping `u32`, so the index has to be split across two elements - written into one, as it used to be, indexes
/// `2^32` apart derived the same key. The high word sits in the element that was the constant `0`, so every index
/// below `2^32` keeps its old path, and its old key, byte for byte.
///
/// The full 64 bits are what keep `GetScriptOffset` safe: it hands the host the random base index of the sender
/// offset keys that blind each reply, and two replies whose bases name the same key can be differenced to strip that
/// blinding. With 32 effective bits a host finds such a pair in about `2^16` calls, with nothing on the screen; with
/// 64 it does not. See `minotari_ledger_wallet_common::script_offset::sender_offset_base_index`.
///
/// The account element still wraps modulo `2^32`.
pub fn derive_from_bip32_key(
    u64_account: u64,
    u64_index: u64,
    u64_key_type: KeyType,
) -> Result<RistrettoSecretKey, AppSW> {
    let account = u64_to_string(u64_account);
    let index_high = u64_to_string(u64_index >> 32);
    let index_low = u64_to_string(u64_index & 0xFFFF_FFFF);
    let key_type = u64_to_string(u64_key_type.as_byte() as u64);

    let mut bip32_path = "m/44'/".to_string();
    bip32_path.push_str(&BIP32_COIN_TYPE.to_string());
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&account);
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&index_high);
    bip32_path.push_str(&"/");
    bip32_path.push_str(&index_low);
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&key_type);
    let path: [u32; 6] = make_bip32_path(bip32_path.as_bytes());

    match get_raw_key_hash(&path) {
        Ok(val) => get_key_from_uniform_bytes(&val),
        Err(e) => {
            let mut msg = "".to_string();
            msg.push_str("Err: raw key >>...");

            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                SingleMessage::new(&msg).show_and_wait();
                SingleMessage::new(&e).show_and_wait();
            }

            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new().text(&msg).show(false);
                NbglStatus::new().text(&e).show(false);
            }
            return Err(AppSW::KeyDeriveFail);
        },
    }
}

/// Get a 32 byte secret key from 64 uniform bytes
pub fn get_key_from_uniform_bytes(bytes: &Zeroizing<[u8; 64]>) -> Result<RistrettoSecretKey, AppSW> {
    match RistrettoSecretKey::from_uniform_bytes(bytes.as_ref()) {
        Ok(val) => Ok(val),
        Err(e) => {
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                MessageScroller::new(&format!(
                    "Err: key conversion {:?}. Length: {:?}",
                    e.to_string(),
                    &bytes.len()
                ))
                .event_loop();
                SingleMessage::new(&format!("Error Length: {:?}", &bytes.len())).show_and_wait();
            }

            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!(
                        "Err: key conversion {:?}. Length: {:?}",
                        e.to_string(),
                        &bytes.len()
                    ))
                    .show(false);
            }
            return Err(AppSW::KeyDeriveFromUniform);
        },
    }
}

/// Get a 32 byte secret key from 32 canonical bytes
pub fn get_key_from_canonical_bytes<T: ByteArray>(bytes: &[u8]) -> Result<T, AppSW> {
    match T::from_canonical_bytes(bytes) {
        Ok(val) => Ok(val),
        Err(e) => {
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                MessageScroller::new(&format!(
                    "Err: key conversion {:?}. Length: {:?}",
                    e.to_string(),
                    &bytes.len()
                ))
                .event_loop();
                SingleMessage::new(&format!("Error Length: {:?}", &bytes.len())).show_and_wait();
            }

            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!(
                        "Err: key conversion {:?}. Length: {:?}",
                        e.to_string(),
                        &bytes.len()
                    ))
                    .show(false);
            }

            return Err(AppSW::KeyDeriveFromCanonical);
        },
    }
}

/// Get the domain separated alpha key hasher
pub fn alpha_hasher(
    alpha: RistrettoSecretKey,
    blinding_factor: RistrettoSecretKey,
) -> Result<RistrettoSecretKey, AppSW> {
    let mut raw_key_hashed = Zeroizing::new([0u8; 64]);
    DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label("script key")
        .chain(blinding_factor.as_bytes())
        .finalize_into(raw_key_hashed.as_mut().into());
    let private_key = get_key_from_uniform_bytes(&raw_key_hashed)?;

    Ok(private_key + alpha)
}

/// Get a uniform random `u64` from the device RNG.
///
/// Used to pick key indexes that the host cannot influence.
pub fn get_random_u64() -> u64 {
    let mut raw_bytes = [0u8; 8];
    LedgerRng.fill_bytes(&mut raw_bytes);
    u64::from_le_bytes(raw_bytes)
}

/// Get a uniform random nonce
pub fn get_random_nonce() -> Result<RistrettoSecretKey, AppSW> {
    let mut raw_bytes = [0u8; 64];
    LedgerRng.fill_bytes(&mut raw_bytes);
    if raw_bytes == [0u8; 64] {
        return Err(AppSW::RandomNonceFail);
    }
    match RistrettoSecretKey::from_uniform_bytes(&raw_bytes) {
        Ok(val) => Ok(val),
        Err(e) => {
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                MessageScroller::new(&format!("Err: nonce conversion {:?}", e.to_string())).event_loop();
                SingleMessage::new(&e.to_string()).show_and_wait();
            }

            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!("Err: nonce conversion {:?}", e.to_string()))
                    .show(false);
            }

            Err(AppSW::KeyDeriveFromUniform)
        },
    }
}

hash_domain!(TransactionHashDomain, "com.tari.base_layer.core.transactions", 0);
