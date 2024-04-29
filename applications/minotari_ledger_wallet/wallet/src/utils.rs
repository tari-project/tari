// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

use blake2::{Blake2b, Digest};
use digest::consts::U64;
use ledger_device_sdk::{
    ecc::{bip32_derive, make_bip32_path, CurvesId, CxError},
    io::SyscallError,
    ui::gadgets::SingleMessage,
};
use tari_crypto::{
    hash_domain,
    hashing::DomainSeparatedHasher,
    keys::SecretKey,
    ristretto::RistrettoSecretKey,
    tari_utilities::ByteArray,
};
use zeroize::Zeroizing;

use crate::{
    alloc::string::{String, ToString},
    AppSW,
    KeyType,
    BIP32_COIN_TYPE,
};

hash_domain!(LedgerHashDomain, "com.tari.minotari_ledger_wallet", 0);
hash_domain!(KeyManagerHashingDomain, "com.tari.base_layer.key_manager", 1);

/// BIP32 path stored as an array of [`u32`].
///
/// # Generic arguments
///
/// * `S` - Maximum possible path length, i.e. the capacity of the internal buffer.
pub struct Bip32Path<const S: usize = 10> {
    buffer: [u32; S],
    len: usize,
}

impl AsRef<[u32]> for Bip32Path {
    fn as_ref(&self) -> &[u32] {
        &self.buffer[..self.len]
    }
}

impl<const S: usize> Default for Bip32Path<S> {
    fn default() -> Self {
        Self {
            buffer: [0u32; S],
            len: 0,
        }
    }
}

impl<const S: usize> TryFrom<&[u8]> for Bip32Path<S> {
    type Error = AppSW;

    /// Constructs a [`Bip32Path`] from a given byte array.
    ///
    /// This method will return an error in the following cases:
    /// - the input array is empty,
    /// - the number of bytes in the input array is not a multiple of 4,
    /// - the input array exceeds the capacity of the [`Bip32Path`] internal buffer.
    ///
    /// # Arguments
    ///
    /// * `data` - Encoded BIP32 path. First byte is the length of the path, as encoded by ragger.
    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let input_path_len = (data.len() - 1) / 4;
        // Check data length
        if data.is_empty() // At least the length byte is required
            || (input_path_len > S)
            || (data[0] as usize * 4 != data.len() - 1)
        {
            return Err(AppSW::WrongApduLength);
        }

        let mut path = [0; S];
        for (chunk, p) in data[1..].chunks(4).zip(path.iter_mut()) {
            *p = u32::from_be_bytes(chunk.try_into().unwrap());
        }

        Ok(Self {
            buffer: path,
            len: input_path_len,
        })
    }
}

/// Convert a u64 to a string without using the standard library
pub fn u64_to_string(number: u64) -> String {
    let mut buffer = [0u8; 20]; // Maximum length for a 64-bit integer (including null terminator)
    let mut pos = 0;

    if number == 0 {
        buffer[pos] = b'0';
        pos += 1;
    } else {
        let mut num = number;

        let mut digits = [0u8; 20];
        let mut num_digits = 0;

        while num > 0 {
            digits[num_digits] = b'0' + (num % 10) as u8;
            num /= 10;
            num_digits += 1;
        }

        while num_digits > 0 {
            num_digits -= 1;
            buffer[pos] = digits[num_digits];
            pos += 1;
        }
    }

    String::from_utf8_lossy(&buffer[..pos]).to_string()
}

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

// Get a raw 32 byte key hash from the BIP32 path.
// - The wrapper function for the syscall `os_perso_derive_node_bip32`, `bip32_derive`, requires a 96 byte buffer when
//   called with `CurvesId::Ed25519` as it checks the consistency of the curve choice and key length in order to prevent
//   the underlying syscall from panicking.
// - The syscall `os_perso_derive_node_bip32` returns 96 bytes as:
//     private key: 64 bytes
//     chain: 32 bytes
//   Example:
//     d8a57c1be0c52e9643485e77aac56d72fa6c4eb831466c2abd2d320c82d3d14929811c598c13d431bad433e037dbd97265492cea42bc2e3aad15440210a20a2d0000000000000000000000000000000000000000000000000000000000000000
//  - This function applies domain separated hashing to the 64 byte private key of the returned buffer to get 32
//    uniformly distributed random bytes.
fn get_raw_key_hash(path: &[u32]) -> Result<Zeroizing<[u8; 64]>, String> {
    let mut key_buffer = Zeroizing::new([0u8; 96]);
    const BIP32_KEY_LENGTH: usize = 64;
    let raw_key_64 = match bip32_derive(CurvesId::Ed25519, path, key_buffer.as_mut(), Some(&mut [])) {
        Ok(_) => {
            let binding = &key_buffer.as_ref()[..BIP32_KEY_LENGTH];
            let mut key_bytes = Zeroizing::new([0u8; BIP32_KEY_LENGTH]);
            // `copy_from_slice` will not panic as the length of the slice is equal to the length of the array
            key_bytes.as_mut().copy_from_slice(binding);
            key_bytes
        },
        Err(e) => return Err(cx_error_to_string(e)),
    };

    let mut raw_key_hashed = Zeroizing::new([0u8; 64]);
    DomainSeparatedHasher::<Blake2b<U64>, LedgerHashDomain>::new_with_label("raw_key")
        .chain(&raw_key_64.as_ref())
        .finalize_into(raw_key_hashed.as_mut().into());

    Ok(raw_key_hashed)
}

/// Get a raw 32 byte key hash from the BIP32 path. In cas of an error, display an interactive message on the device.
pub fn get_raw_key(path: &[u32]) -> Result<Zeroizing<[u8; 64]>, SyscallError> {
    match get_raw_key_hash(&path) {
        Ok(val) => Ok(val),
        Err(e) => {
            let mut msg = "".to_string();
            msg.push_str("Err: raw key >>...");
            SingleMessage::new(&msg).show_and_wait();
            SingleMessage::new(&e).show_and_wait();
            Err(SyscallError::InvalidParameter.into())
        },
    }
}

pub fn get_key_from_uniform_bytes(bytes: &[u8]) -> Result<RistrettoSecretKey, AppSW> {
    match RistrettoSecretKey::from_uniform_bytes(bytes) {
        Ok(val) => Ok(val),
        Err(e) => {
            SingleMessage::new(&format!(
                "Err: key conversion {:?}. Length: {:?}",
                e.to_string(),
                &bytes.len()
            ))
            .show_and_wait();
            SingleMessage::new(&format!("Error Length: {:?}", &bytes.len())).show_and_wait();
            return Err(AppSW::KeyDeriveFail);
        },
    }
}

pub fn get_key_from_canonical_bytes(bytes: &[u8]) -> Result<RistrettoSecretKey, AppSW> {
    match RistrettoSecretKey::from_canonical_bytes(bytes) {
        Ok(val) => Ok(val),
        Err(e) => {
            SingleMessage::new(&format!(
                "Err: key conversion {:?}. Length: {:?}",
                e.to_string(),
                &bytes.len()
            ))
            .show_and_wait();
            SingleMessage::new(&format!("Error Length: {:?}", &bytes.len())).show_and_wait();
            return Err(AppSW::KeyDeriveFail);
        },
    }
}

pub fn mask_a(alpha: RistrettoSecretKey, commitment: RistrettoSecretKey) -> Result<RistrettoSecretKey, AppSW> {
    let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerHashingDomain>::new_with_label("script key");
    let hasher = hasher.chain(commitment.as_bytes()).finalize();
    let private_key = get_key_from_uniform_bytes(hasher.as_ref())?;

    Ok(private_key + alpha)
}

pub fn derive_from_bip32_key(
    u64_account: u64,
    u64_index: u64,
    u64_key_type: KeyType,
) -> Result<RistrettoSecretKey, AppSW> {
    let account = u64_to_string(u64_account);
    let index = u64_to_string(u64_index);
    let key_type = u64_to_string(u64_key_type.to_byte() as u64);

    let mut bip32_path = "m/44'/".to_string();
    bip32_path.push_str(&BIP32_COIN_TYPE.to_string());
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&account);
    bip32_path.push_str(&"'/0/");
    bip32_path.push_str(&index);
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&key_type);
    let path: [u32; 6] = make_bip32_path(bip32_path.as_bytes());

    match get_raw_key(&path) {
        Ok(val) => get_key_from_uniform_bytes(&val.as_ref()),
        Err(e) => {
            SingleMessage::new(&format!("Key error {:?}", e)).show_and_wait();
            return Err(AppSW::KeyDeriveFail);
        },
    }
}
