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
use minotari_ledger_wallet_common::codec::{
    ComAndPubSigReply,
    Decode,
    GetScriptSignatureDerivedRequest,
    GetScriptSignatureManagedRequest,
    ScriptSignatureCommon,
};

use crate::{
    alloc::string::ToString,
    crypto::{
        commitment::PedersenCommitment,
        commitment_and_public_key_signature::CommitmentAndPublicKeySignature,
        commitment_factory::PedersenCommitmentFactory,
        keys::{RistrettoPublicKey, RistrettoSecretKey},
    },
    hashing::DomainSeparatedConsensusHasher,
    utils::{
        alpha_hasher,
        derive_from_bip32_key,
        get_key_from_canonical_bytes,
        get_random_nonce,
        TransactionHashDomain,
    },
    wire::{invalid_data_length, reply},
    AppSW,
    KeyType,
    STATIC_SPEND_INDEX,
};

pub fn handler_get_script_signature_managed(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetScriptSignatureManagedRequest::decode(data).map_err(|_| invalid_data_length())?;

    let (account, network, txi_version, value, commitment_private_key, commitment, script_message) =
        extract_common_values(&request.common)?;

    let branch = KeyType::from_branch_key(request.branch)?;
    let script_private_key = derive_from_bip32_key(account, request.index, branch)?;
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

    reply_script_signature(comm, &script_signature);

    Ok(())
}

pub fn handler_get_script_signature_derived(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let request = GetScriptSignatureDerivedRequest::decode(data).map_err(|_| invalid_data_length())?;

    let (account, network, txi_version, value, commitment_private_key, commitment, script_message) =
        extract_common_values(&request.common)?;

    let alpha = derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?;
    let blinding_factor: RistrettoSecretKey =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(request.blinding_factor)?.into();
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

    reply_script_signature(comm, &script_signature);

    Ok(())
}

fn reply_script_signature(comm: &mut Comm, signature: &CommitmentAndPublicKeySignature) {
    reply(
        comm,
        &ComAndPubSigReply::new(
            signature.ephemeral_commitment().as_array(),
            signature.ephemeral_pubkey().as_array(),
            signature.u_a().as_array(),
            signature.u_x().as_array(),
            signature.u_y().as_array(),
        ),
    );
}

/// Turn the framed common prefix into the values the signature needs.
///
/// The canonical checks run in the order they always have - value, commitment private key, commitment - because the
/// first one to fail decides which status word the host sees.
fn extract_common_values<'a>(
    common: &ScriptSignatureCommon<'a>,
) -> Result<
    (
        u64,
        u64,
        u64,
        RistrettoSecretKey,
        RistrettoSecretKey,
        PedersenCommitment,
        &'a [u8; 32],
    ),
    AppSW,
> {
    let value: RistrettoSecretKey = get_key_from_canonical_bytes::<RistrettoSecretKey>(common.value)?.into();
    let commitment_private_key: RistrettoSecretKey =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(common.commitment_private_key)?.into();

    let commitment: PedersenCommitment = get_key_from_canonical_bytes(common.commitment)?;

    Ok((
        common.account,
        common.network,
        common.txi_version,
        value,
        commitment_private_key,
        commitment,
        common.message,
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
    script_message: &[u8; 32],
) -> Result<CommitmentAndPublicKeySignature, AppSW> {
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

    let factory = PedersenCommitmentFactory::default();

    let ephemeral_commitment = factory.commit(&r_x, &r_a);
    let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&r_y);

    let challenge = finalize_script_signature_challenge(
        txi_version,
        network,
        &ephemeral_commitment,
        &ephemeral_pubkey,
        &script_public_key,
        &commitment,
        script_message,
    );

    match CommitmentAndPublicKeySignature::sign(
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
        Err(_e) => {
            let error_string = "Invalid Challenge".to_string();
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                SingleMessage::new(&format!("Signing error: {}", error_string)).show_and_wait();
            }

            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!("Signing error: {}", error_string))
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
