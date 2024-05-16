// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

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
use zeroize::Zeroizing;

use crate::{
    alloc::string::ToString,
    hashing::DomainSeparatedConsensusHasher,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes, TransactionHashDomain},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_ALPHA_INDEX,
};

pub fn handler_get_script_signature(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let mut network_bytes = [0u8; 8];
    network_bytes.clone_from_slice(&data[8..16]);
    let network = u64::from_le_bytes(network_bytes);

    let mut txi_version_bytes = [0u8; 8];
    txi_version_bytes.clone_from_slice(&data[16..24]);
    let txi_version = u64::from_le_bytes(txi_version_bytes);

    let alpha = derive_from_bip32_key(account, STATIC_ALPHA_INDEX, KeyType::Alpha)?;
    let blinding_factor: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[24..56])?.into();
    let script_private_key = alpha_hasher(alpha, blinding_factor)?;
    let script_public_key = RistrettoPublicKey::from_secret_key(&script_private_key);

    let value: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[56..88])?.into();
    let spend_private_key: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[88..120])?.into();

    let commitment: PedersenCommitment = get_key_from_canonical_bytes(&data[120..152])?;

    let mut script_message = [0u8; 32];
    script_message.clone_from_slice(&data[152..184]);

    let r_a = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let r_x = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let r_y = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;

    let factory = ExtendedPedersenCommitmentFactory::default();

    let ephemeral_commitment = factory.commit(&r_x, &r_a);
    let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&r_y);

    let challenge = finalize_script_signature_challenge(
        txi_version,
        network,
        &ephemeral_commitment,
        &ephemeral_pubkey,
        &script_public_key,
        &commitment,
        &script_message,
    );

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

fn finalize_script_signature_challenge(
    _version: u64,
    network: u64,
    ephemeral_commitment: &PedersenCommitment,
    ephemeral_pubkey: &RistrettoPublicKey,
    script_public_key: &RistrettoPublicKey,
    commitment: &PedersenCommitment,
    message: &[u8; 32],
) -> [u8; 64] {
    DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("script_challenge", network)
        .chain(ephemeral_commitment)
        .chain(ephemeral_pubkey)
        .chain(script_public_key)
        .chain(commitment)
        .chain(message)
        .finalize()
        .into()
}
