// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

use ledger_device_sdk::io::Comm;
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::gadgets::SingleMessage;
use tari_crypto::{
    hash_domain,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    signatures::SchnorrSignature,
    tari_utilities::ByteArray,
};

use crate::{
    alloc::string::ToString,
    utils::{derive_from_bip32_key, get_random_nonce},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
};

hash_domain!(CheckSigHashDomain, "com.tari.script.check_sig", 1);

/// The type used for `CheckSig`, `CheckMultiSig`, and related opcodes' signatures
pub type CheckSigSchnorrSignature = SchnorrSignature<RistrettoPublicKey, RistrettoSecretKey, CheckSigHashDomain>;

pub fn handler_get_raw_schnorr_signature(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    if data.len() != 104 {
        SingleMessage::new("Invalid data length").show_and_wait();
        return Err(AppSW::WrongApduLength);
    }

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let mut private_key_index_bytes = [0u8; 8];
    private_key_index_bytes.clone_from_slice(&data[8..16]);
    let private_key_index = u64::from_le_bytes(private_key_index_bytes);

    let mut private_key_type_bytes = [0u8; 8];
    private_key_type_bytes.clone_from_slice(&data[16..24]);
    let private_key_type = KeyType::from_branch_key(u64::from_le_bytes(private_key_type_bytes))?;

    let private_key = derive_from_bip32_key(account, private_key_index, private_key_type)?;

    let mut private_nonce_index_bytes = [0u8; 8];
    private_nonce_index_bytes.clone_from_slice(&data[24..32]);
    let private_nonce_index = u64::from_le_bytes(private_nonce_index_bytes);

    let mut nonce_key_type_bytes = [0u8; 8];
    nonce_key_type_bytes.clone_from_slice(&data[32..40]);
    let nonce_key_type = KeyType::from_branch_key(u64::from_le_bytes(nonce_key_type_bytes))?;

    let private_nonce = derive_from_bip32_key(account, private_nonce_index, nonce_key_type)?;

    let mut challenge_bytes = [0u8; 64];
    challenge_bytes.clone_from_slice(&data[40..104]);

    let signature = match RistrettoSchnorr::sign_raw_uniform(&private_key, private_nonce.clone(), &challenge_bytes) {
        Ok(sig) => sig,
        Err(e) => {
            SingleMessage::new(&format!("Signing error: {:?}", e.to_string())).show_and_wait();
            return Err(AppSW::RawSchnorrSignatureFail);
        },
    };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&signature.get_public_nonce().to_vec());
    comm.append(&signature.get_signature().to_vec());
    comm.reply_ok();

    Ok(())
}

pub fn handler_get_script_schnorr_signature(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    if data.len() != 56 {
        SingleMessage::new("Invalid data length").show_and_wait();
        return Err(AppSW::WrongApduLength);
    }

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let mut private_key_index_bytes = [0u8; 8];
    private_key_index_bytes.clone_from_slice(&data[8..16]);
    let private_key_index = u64::from_le_bytes(private_key_index_bytes);

    let mut private_key_type_bytes = [0u8; 8];
    private_key_type_bytes.clone_from_slice(&data[16..24]);
    let key_type = u64::from_le_bytes(private_key_type_bytes);
    let private_key_type = KeyType::from_branch_key(key_type)?;

    let private_key = derive_from_bip32_key(account, private_key_index, private_key_type)?;

    let mut nonce_bytes = [0u8; 32];
    nonce_bytes.clone_from_slice(&data[24..56]);

    let random_nonce = get_random_nonce()?.clone();
    let signature =
        match CheckSigSchnorrSignature::sign_with_nonce_and_message(&private_key, random_nonce, &nonce_bytes) {
            Ok(sig) => sig,
            Err(e) => {
                SingleMessage::new(&format!("Signing error:",)).show_and_wait();
                SingleMessage::new(&format!("{}", e.to_string())).show_and_wait();
                return Err(AppSW::SchnorrSignatureFail);
            },
        };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&signature.get_public_nonce().to_vec());
    comm.append(&signature.get_signature().to_vec());
    comm.reply_ok();

    Ok(())
}
