// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

use ledger_device_sdk::{io::Comm, ui::gadgets::SingleMessage};
use tari_crypto::{
    keys::PublicKey,
    ristretto::{
        pedersen::{extended_commitment_factory::ExtendedPedersenCommitmentFactory, PedersenCommitment},
        RistrettoComAndPubSig,
        RistrettoPublicKey,
        RistrettoSecretKey,
    },
};

use crate::{
    alloc::string::ToString,
    utils::{derive_from_bip32_key, finalize_metadata_signature_challenge, get_key_from_canonical_bytes},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
};

pub fn handler_get_metadata_signature(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    SingleMessage::new(&"got data").show_and_wait();

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);
    SingleMessage::new(&"got account").show_and_wait();

    let mut network_bytes = [0u8; 8];
    network_bytes.clone_from_slice(&data[8..16]);
    let network = u64::from_le_bytes(network_bytes);
    SingleMessage::new(&"got network").show_and_wait();

    let mut txo_version_bytes = [0u8; 8];
    txo_version_bytes.clone_from_slice(&data[16..24]);
    let txo_version = u64::from_le_bytes(txo_version_bytes);
    SingleMessage::new(&"got txo version").show_and_wait();

    let mut ephemeral_private_nonce_index_bytes = [0u8; 8];
    ephemeral_private_nonce_index_bytes.clone_from_slice(&data[24..32]);
    let ephemeral_private_nonce_index = u64::from_le_bytes(ephemeral_private_nonce_index_bytes);
    SingleMessage::new(&"got eph nonce").show_and_wait();

    let mut sender_offset_key_index_bytes = [0u8; 8];
    sender_offset_key_index_bytes.clone_from_slice(&data[32..40]);
    let sender_offset_key_index = u64::from_le_bytes(sender_offset_key_index_bytes);
    SingleMessage::new(&"got offsets").show_and_wait();

    let commitment: PedersenCommitment = get_key_from_canonical_bytes(&data[40..72])?;
    SingleMessage::new(&"gen commit").show_and_wait();
    let ephemeral_commitment: PedersenCommitment = get_key_from_canonical_bytes(&data[72..104])?;
    SingleMessage::new(&"gen eph commit").show_and_wait();

    let mut metadata_signature_message = [0u8; 32];
    metadata_signature_message.clone_from_slice(&data[104..136]);
    SingleMessage::new(&"got message").show_and_wait();

    let ephemeral_private_key = derive_from_bip32_key(account, ephemeral_private_nonce_index, KeyType::Nonce)?;
    let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&ephemeral_private_key);
    SingleMessage::new(&"derived eph keys").show_and_wait();

    let sender_offset_private_key = derive_from_bip32_key(account, sender_offset_key_index, KeyType::SenderOffset)?;
    let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_private_key);
    SingleMessage::new(&"derived offset keys").show_and_wait();

    let challenge = finalize_metadata_signature_challenge(
        txo_version,
        network,
        &sender_offset_public_key,
        &ephemeral_commitment,
        &ephemeral_pubkey,
        &commitment,
        &metadata_signature_message,
    );
    SingleMessage::new(&"challenge").show_and_wait();

    let factory = ExtendedPedersenCommitmentFactory::default();
    SingleMessage::new(&"factory").show_and_wait();

    let metadata_signature = match RistrettoComAndPubSig::sign(
        &RistrettoSecretKey::default(),
        &RistrettoSecretKey::default(),
        &sender_offset_private_key,
        &RistrettoSecretKey::default(),
        &RistrettoSecretKey::default(),
        &ephemeral_private_key,
        &challenge,
        &factory,
    ) {
        Ok(sig) => sig,
        Err(e) => {
            SingleMessage::new(&format!("Signing error: {:?}", e.to_string())).show_and_wait();
            return Err(AppSW::ScriptSignatureFail);
        },
    };
    SingleMessage::new(&"complete").show_and_wait();

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&metadata_signature.to_vec());
    comm.reply_ok();

    Ok(())
}
