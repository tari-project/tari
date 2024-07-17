// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;
use core::ops::Deref;

use blake2::Blake2b;
use digest::consts::U64;
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
use tari_hashing::TransactionHashDomain;
use zeroize::Zeroizing;

use crate::{
    alloc::string::ToString,
    hashing::DomainSeparatedConsensusHasher,
    utils::{derive_from_bip32_key, get_key_from_canonical_bytes},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
};

pub fn handler_get_one_sided_metadata_signature(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let mut network_bytes = [0u8; 8];
    network_bytes.clone_from_slice(&data[8..16]);
    let network = u64::from_le_bytes(network_bytes);

    let mut txo_version_bytes = [0u8; 8];
    txo_version_bytes.clone_from_slice(&data[16..24]);
    let txo_version = u64::from_le_bytes(txo_version_bytes);

    let mut spend_key_index_bytes = [0u8; 8];
    spend_key_index_bytes.clone_from_slice(&data[24..32]);
    let spend_key_index = u64::from_le_bytes(spend_key_index_bytes);
    let spend_private_key = derive_from_bip32_key(account, spend_key_index, KeyType::Spend)?;

    let mut sender_offset_key_index_bytes = [0u8; 8];
    sender_offset_key_index_bytes.clone_from_slice(&data[32..40]);
    let sender_offset_key_index = u64::from_le_bytes(sender_offset_key_index_bytes);

    let sender_offset_private_key =
        derive_from_bip32_key(account, sender_offset_key_index, KeyType::OneSidedSenderOffset)?;
    let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_private_key);

    let value: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[40..72])?.into();

    let r_a = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let r_x = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let ephemeral_private_key = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;

    let factory = ExtendedPedersenCommitmentFactory::default();

    let commitment = factory.commit(&spend_private_key, value.deref());
    let ephemeral_commitment = factory.commit(&r_x, &r_a);
    let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&ephemeral_private_key);

    let mut metadata_signature_message = [0u8; 32];
    metadata_signature_message.clone_from_slice(&data[72..104]);

    let challenge = finalize_metadata_signature_challenge(
        txo_version,
        network,
        &sender_offset_public_key,
        &ephemeral_commitment,
        &ephemeral_pubkey,
        &commitment,
        &metadata_signature_message,
    );

    let metadata_signature = match RistrettoComAndPubSig::sign(
        &value,
        &spend_private_key,
        &sender_offset_private_key,
        &r_a,
        &r_x,
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

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&metadata_signature.to_vec());
    comm.reply_ok();

    Ok(())
}

fn finalize_metadata_signature_challenge(
    _version: u64,
    network: u64,
    sender_offset_public_key: &RistrettoPublicKey,
    ephemeral_commitment: &PedersenCommitment,
    ephemeral_pubkey: &RistrettoPublicKey,
    commitment: &PedersenCommitment,
    message: &[u8; 32],
) -> [u8; 64] {
    let challenge =
        DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("metadata_signature", network)
            .chain(ephemeral_pubkey)
            .chain(ephemeral_commitment)
            .chain(sender_offset_public_key)
            .chain(commitment)
            .chain(&message)
            .finalize();

    challenge.into()
}
