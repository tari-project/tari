// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

use ledger_device_sdk::{io::Comm, random::Random, ui::gadgets::SingleMessage};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::PublicKey,
    ristretto::{
        pedersen::{extended_commitment_factory::ExtendedPedersenCommitmentFactory, PedersenCommitment},
        RistrettoComAndPubSig,
        RistrettoPublicKey,
        RistrettoSecretKey,
    },
};
use zeroize::Zeroizing;

use crate::{
    alloc::string::ToString,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_ALPHA_INDEX,
};

pub fn handler_get_script_signature_from_challenge(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let alpha = derive_from_bip32_key(account, STATIC_ALPHA_INDEX, KeyType::Alpha)?;
    let blinding_factor: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[8..40])?.into();
    let script_private_key = alpha_hasher(alpha, blinding_factor)?;
    let script_public_key = RistrettoPublicKey::from_secret_key(&script_private_key);

    let value: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[40..72])?.into();
    let spend_private_key: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[72..104])?.into();

    let commitment: PedersenCommitment = get_key_from_canonical_bytes(&data[104..136])?;

    let mut challenge = [0u8; 64];
    challenge.clone_from_slice(&data[136..200]);

    let r_a = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let r_x = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let r_y = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;

    let factory = ExtendedPedersenCommitmentFactory::default();

    let script_signature = match RistrettoComAndPubSig::sign(
        &value,
        &spend_private_key,
        &script_private_key,
        &r_a,
        &r_x,
        &r_y,
        &challenge,
        &factory,
    ) {
        Ok(sig) => sig,
        Err(e) => {
            SingleMessage::new(&format!("Signing error: {:?}", e.to_string())).show_and_wait();
            return Err(AppSW::ScriptSignatureFail);
        },
    };

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_signature.to_vec());
    comm.reply_ok();

    Ok(())
}
