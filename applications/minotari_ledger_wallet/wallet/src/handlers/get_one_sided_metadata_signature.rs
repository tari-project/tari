// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::{format, string::String};
use core::ops::Deref;

use blake2::Blake2b;
use digest::consts::{U32, U64};
use ledger_device_sdk::{
    io::Comm,
    random::Random,
    ui::{
        bitmaps::{CROSSMARK, EYE, VALIDATE_14},
        gadgets::{Field, MultiFieldReview, SingleMessage},
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    hashing::DomainSeparatedHasher,
    keys::PublicKey,
    ristretto::{
        pedersen::{extended_commitment_factory::ExtendedPedersenCommitmentFactory, PedersenCommitment},
        RistrettoComAndPubSig,
        RistrettoPublicKey,
        RistrettoSecretKey,
    },
    tari_utilities::ByteArray,
};
use tari_hashing::{KeyManagerTransactionsHashDomain, TransactionHashDomain};
use zeroize::Zeroizing;

use crate::{
    alloc::string::ToString,
    hashing::DomainSeparatedConsensusHasher,
    utils::{derive_from_bip32_key, get_key_from_canonical_bytes, get_key_from_uniform_bytes},
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

    let mut sender_offset_key_index_bytes = [0u8; 8];
    sender_offset_key_index_bytes.clone_from_slice(&data[24..32]);
    let sender_offset_key_index = u64::from_le_bytes(sender_offset_key_index_bytes);

    let sender_offset_private_key =
        derive_from_bip32_key(account, sender_offset_key_index, KeyType::OneSidedSenderOffset)?;
    let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_private_key);

    let mut value_bytes = [0u8; 8];
    value_bytes.clone_from_slice(&data[32..40]);
    let value_u64 = u64::from_le_bytes(value_bytes);
    let value = Minotari::new(u64::from_le_bytes(value_bytes));
    let value_as_private_key: Zeroizing<RistrettoSecretKey> = Zeroizing::new(value_u64.into());

    let commitment_mask: Zeroizing<RistrettoSecretKey> =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[40..72])?.into();

    let r_a = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let r_x = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;
    let ephemeral_private_key = derive_from_bip32_key(account, u32::random().into(), KeyType::Nonce)?;

    let factory = ExtendedPedersenCommitmentFactory::default();

    let commitment = factory.commit(&commitment_mask, value_as_private_key.deref());
    let ephemeral_commitment = factory.commit(&r_x, &r_a);
    let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&ephemeral_private_key);

    let receiver_public_spend_key: Zeroizing<RistrettoPublicKey> =
        get_key_from_canonical_bytes::<RistrettoPublicKey>(&data[72..104])?.into();

    let mut metadata_signature_message_common = [0u8; 32];
    metadata_signature_message_common.clone_from_slice(&data[104..136]);

    let script = generate_tari_script(network, &commitment_mask, &receiver_public_spend_key)?;
    let metadata_signature_message =
        metadata_signature_message_from_script_and_common(network, &script, &metadata_signature_message_common);

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
        &value_as_private_key,
        &commitment_mask,
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

    let fields = [Field {
        name: "Amount",
        value: &format!("{}", value.to_string()),
    }];
    let review = MultiFieldReview::new(
        &fields,
        &["Review ", "Transaction"],
        Some(&EYE),
        "Approve",
        Some(&VALIDATE_14),
        "Reject",
        Some(&CROSSMARK),
    );

    match review.show() {
        true => {
            comm.append(&[RESPONSE_VERSION]); // version
            comm.append(&metadata_signature.to_vec());
            comm.reply_ok();
        },
        false => {
            return Err(AppSW::UserCancelled);
        },
    }

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

fn metadata_signature_message_from_script_and_common(network: u64, script: &[u8; 32], common: &[u8; 32]) -> [u8; 32] {
    DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message", network)
        .chain(script)
        .chain(common)
        .finalize()
        .into()
}

fn generate_tari_script(
    network: u64,
    private_key: &Zeroizing<RistrettoSecretKey>,
    spend_key: &Zeroizing<RistrettoPublicKey>,
) -> Result<[u8; 32], AppSW> {
    let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label("script key");
    let hasher = hasher.chain(private_key.as_bytes()).finalize();
    let private_key = get_key_from_uniform_bytes(hasher.as_ref())?;
    let public_key = Zeroizing::new(RistrettoPublicKey::from_secret_key(&private_key));
    let public_key = Zeroizing::new(spend_key.deref() + public_key.deref());

    // Push into push_pub tari script
    // TODO CREATE TARISCRIPT
    let script: [u8; 32] = public_key.as_bytes();

    DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message", network)
        .chain(&script)
        .finalize()
}

struct Minotari(pub u64);

impl Minotari {
    fn new(value: u64) -> Self {
        Self(value)
    }

    fn to_string(&self) -> String {
        if self.0 < 1_000_000 {
            format!("{} uT", self.0)
        } else {
            let value = self.0 as f64 / 1_000_000.0;
            format!("{:.2} T", value)
        }
    }
}
