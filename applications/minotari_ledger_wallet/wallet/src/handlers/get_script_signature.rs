// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

use blake2::Blake2b;
use digest::consts::U64;
use ledger_device_sdk::io::Comm;
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::NbglStatus;
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::gadgets::SingleMessage;
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

use crate::{
    alloc::string::ToString,
    hashing::DomainSeparatedConsensusHasher,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes, get_random_nonce},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_SPEND_INDEX,
};

pub fn handler_get_script_signature_managed(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    if data.len() != 168 {
        #[cfg(not(any(target_os = "stax", target_os = "flex")))]
        {
            SingleMessage::new("Invalid data length").show_and_wait();
        }
        #[cfg(any(target_os = "stax", target_os = "flex"))]
        {
            NbglStatus::new().text(&"Invalid data length").show(false);
        }
        return Err(AppSW::WrongApduLength);
    }

    let (account, network, txi_version, value, commitment_private_key, commitment, script_message) =
        extract_common_values(data)?;

    let mut branch = [0u8; 8];
    branch.clone_from_slice(&data[152..160]);
    let branch = KeyType::from_branch_key(u64::from_le_bytes(branch))?;
    let mut index = [0u8; 8];
    index.clone_from_slice(&data[160..168]);
    let index = u64::from_le_bytes(index);
    let script_private_key = derive_from_bip32_key(account, index, branch)?;
    let script_public_key = RistrettoPublicKey::from_secret_key(&script_private_key);

    let script_signature = get_script_signature(
        txi_version,
        network,
        value,
        commitment_private_key,
        script_private_key,
        script_public_key,
        commitment,
        script_message,
    )?;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_signature.to_vec());
    comm.reply_ok();

    Ok(())
}

pub fn handler_get_script_signature_derived(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    if data.len() != 184 {
        #[cfg(not(any(target_os = "stax", target_os = "flex")))]
        {
            SingleMessage::new("Invalid data length").show_and_wait();
        }
        #[cfg(any(target_os = "stax", target_os = "flex"))]
        {
            NbglStatus::new().text(&"Invalid data length").show(false);
        }
        return Err(AppSW::WrongApduLength);
    }

    let (account, network, txi_version, value, commitment_private_key, commitment, script_message) =
        extract_common_values(data)?;

    let alpha = derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?;
    let blinding_factor: RistrettoSecretKey =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[152..184])?.into();
    let script_private_key = alpha_hasher(alpha, blinding_factor)?;
    let script_public_key = RistrettoPublicKey::from_secret_key(&script_private_key);

    let script_signature = get_script_signature(
        txi_version,
        network,
        value,
        commitment_private_key,
        script_private_key,
        script_public_key,
        commitment,
        script_message,
    )?;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_signature.to_vec());
    comm.reply_ok();

    Ok(())
}

fn extract_common_values(
    data: &[u8],
) -> Result<
    (
        u64,
        u64,
        u64,
        RistrettoSecretKey,
        RistrettoSecretKey,
        PedersenCommitment,
        [u8; 32],
    ),
    AppSW,
> {
    if data.len() < 152 {
        return Err(AppSW::WrongApduLength);
    }
    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&data[0..8]);
    let account = u64::from_le_bytes(account_bytes);

    let mut network_bytes = [0u8; 8];
    network_bytes.clone_from_slice(&data[8..16]);
    let network = u64::from_le_bytes(network_bytes);

    let mut txi_version_bytes = [0u8; 8];
    txi_version_bytes.clone_from_slice(&data[16..24]);
    let txi_version = u64::from_le_bytes(txi_version_bytes);

    let value: RistrettoSecretKey = get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[24..56])?.into();
    let commitment_private_key: RistrettoSecretKey =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[56..88])?.into();

    let commitment: PedersenCommitment = get_key_from_canonical_bytes(&data[88..120])?;

    let mut script_message = [0u8; 32];
    script_message.clone_from_slice(&data[120..152]);

    Ok((
        account,
        network,
        txi_version,
        value,
        commitment_private_key,
        commitment,
        script_message,
    ))
}

fn get_script_signature(
    txi_version: u64,
    network: u64,
    value: RistrettoSecretKey,
    commitment_private_key: RistrettoSecretKey,
    script_private_key: RistrettoSecretKey,
    script_public_key: RistrettoPublicKey,
    commitment: PedersenCommitment,
    script_message: [u8; 32],
) -> Result<RistrettoComAndPubSig, AppSW> {
    let r_a = get_random_nonce()?;
    let r_x = get_random_nonce()?;
    let r_y = get_random_nonce()?;
    if r_a == r_x || r_a == r_y || r_x == r_y {
        #[cfg(not(any(target_os = "stax", target_os = "flex")))]
        {
            SingleMessage::new("Nonces not unique").show_and_wait();
        }
        #[cfg(any(target_os = "stax", target_os = "flex"))]
        {
            NbglStatus::new().text(&"Nonces not unique").show(false);
        }
        return Err(AppSW::ScriptSignatureFail);
    }

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

    match RistrettoComAndPubSig::sign(
        &value,
        &commitment_private_key,
        &script_private_key,
        &r_a,
        &r_x,
        &r_y,
        &challenge,
        &factory,
    ) {
        Ok(sig) => Ok(sig),
        Err(e) => {
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                SingleMessage::new(&format!("Signing error: {:?}", e.to_string())).show_and_wait();
            }
            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!("Signing error: {:?}", e.to_string()))
                    .show(false);
            }
            Err(AppSW::ScriptSignatureFail)
        },
    }
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
