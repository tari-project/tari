// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::{format, string::String, vec::Vec};

use blake2::Blake2b;
use borsh::{io, BorshSerialize};
use digest::{
    consts::{U32, U64},
    Digest,
};
#[cfg(any(target_os = "stax", target_os = "flex"))]
use include_gif::include_gif;
use ledger_device_sdk::io::Comm;
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::{Field, NbglGlyph, NbglReview, NbglStatus};
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::{
    bitmaps::{CROSSMARK, EYE, VALIDATE_14},
    gadgets::{Field, MultiFieldReview, SingleMessage},
};
use minotari_ledger_wallet_common::{
    get_public_spend_key_bytes_from_tari_dual_address,
    tari_dual_address_display,
    TARI_DUAL_ADDRESS_SIZE,
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
    utils::{derive_from_bip32_key, get_key_from_canonical_bytes, get_key_from_uniform_bytes, get_random_nonce},
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

    let mut value_bytes = [0u8; 8];
    value_bytes.clone_from_slice(&data[32..40]);
    let value_u64 = u64::from_le_bytes(value_bytes);
    let value = Minotari::new(u64::from_le_bytes(value_bytes));

    let commitment_mask: RistrettoSecretKey = get_key_from_canonical_bytes::<RistrettoSecretKey>(&data[40..72])?.into();

    let mut receiver_address_bytes = [0u8; TARI_DUAL_ADDRESS_SIZE]; // 67 bytes
    receiver_address_bytes.clone_from_slice(&data[72..139]);

    let receiver_address = match tari_dual_address_display(&receiver_address_bytes) {
        Ok(address) => address,
        Err(e) => {
            #[cfg(not(any(target_os = "stax", target_os = "flex")))]
            {
                SingleMessage::new(&format!("Error: {:?}", e.to_string())).show_and_wait();
            }
            #[cfg(any(target_os = "stax", target_os = "flex"))]
            {
                NbglStatus::new()
                    .text(&format!("Error: {:?}", e.to_string()))
                    .show(false);
            }
            return Err(AppSW::MetadataSignatureFail);
        },
    };

    let mut metadata_signature_message_common = [0u8; 32];
    metadata_signature_message_common.clone_from_slice(&data[139..171]);

    let fields = [
        Field {
            name: "Amount",
            value: &format!("{}", value.to_string()),
        },
        Field {
            name: "Receiver",
            value: &format!("{}", receiver_address),
        },
    ];
    #[cfg(not(any(target_os = "stax", target_os = "flex")))]
    {
        let review = MultiFieldReview::new(
            &fields,
            &["Review ", "Transaction"],
            Some(&EYE),
            "Approve",
            Some(&VALIDATE_14),
            "Reject",
            Some(&CROSSMARK),
        );
        if !review.show() {
            return Err(AppSW::UserCancelled);
        }
    }
    #[cfg(any(target_os = "stax", target_os = "flex"))]
    {
        // Load glyph from 64x64 4bpp gif file with include_gif macro. Creates an NBGL compatible glyph.
        const FERRIS: NbglGlyph = NbglGlyph::from_include(include_gif!("key_64x64.gif", NBGL));
        // Create NBGL review. Maximum number of fields and string buffer length can be customised
        // with constant generic parameters of NbglReview. Default values are 32 and 1024 respectively.
        let review: NbglReview = NbglReview::new()
            .titles("Review transaction\nto send", "", "Sign transaction\nto send")
            .glyph(&FERRIS);

        //
        if !review.show(&fields[0..2]) {
            return Err(AppSW::UserCancelled);
        }
    }

    let value_as_private_key: RistrettoSecretKey = value_u64.into();

    let sender_offset_private_key =
        derive_from_bip32_key(account, sender_offset_key_index, KeyType::OneSidedSenderOffset)?;
    let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_private_key);

    let r_a = get_random_nonce()?;
    let r_x = get_random_nonce()?;
    let ephemeral_private_key = get_random_nonce()?;

    let factory = ExtendedPedersenCommitmentFactory::default();

    let commitment = factory.commit(&commitment_mask, &value_as_private_key);
    let ephemeral_commitment = factory.commit(&r_x, &r_a);
    let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&ephemeral_private_key);

    let receiver_public_spend_key: RistrettoPublicKey =
        match get_public_spend_key_bytes_from_tari_dual_address(&receiver_address_bytes) {
            Ok(bytes) => get_key_from_canonical_bytes::<RistrettoPublicKey>(&bytes)?,
            Err(e) => {
                #[cfg(not(any(target_os = "stax", target_os = "flex")))]
                {
                    SingleMessage::new(&format!("Error: {:?}", e.to_string())).show_and_wait();
                }
                #[cfg(any(target_os = "stax", target_os = "flex"))]
                {
                    NbglStatus::new()
                        .text(&format!("Error: {:?}", e.to_string()))
                        .show(false);
                }
                return Err(AppSW::MetadataSignatureFail);
            },
        };

    let script = tari_script_with_address(&commitment_mask, &receiver_public_spend_key)?;
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
            return Err(AppSW::MetadataSignatureFail);
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

fn metadata_signature_message_from_script_and_common(network: u64, script: &Script, common: &[u8; 32]) -> [u8; 32] {
    DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message", network)
        .chain(script)
        .chain(common)
        .finalize()
        .into()
}

fn tari_script_with_address(
    commitment_mask: &RistrettoSecretKey,
    receiver_public_spend_key: &RistrettoPublicKey,
) -> Result<Script, AppSW> {
    let mut raw_key_hashed = Zeroizing::new([0u8; 64]);
    DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label("script key")
        .chain(commitment_mask.as_bytes())
        .finalize_into(raw_key_hashed.as_mut().into());
    let hashed_commitment_mask = get_key_from_uniform_bytes(&raw_key_hashed)?;
    let hashed_commitment_mask_public_key = RistrettoPublicKey::from_secret_key(&hashed_commitment_mask);
    let stealth_key = receiver_public_spend_key + hashed_commitment_mask_public_key;

    let mut serialized_script: Vec<u8> = stealth_key.as_bytes().to_vec();
    serialized_script.insert(0, 0x7e); // OpCode
    serialized_script.insert(0, 33); // Length

    Ok(Script {
        inner: serialized_script,
    })
}

struct Script {
    pub inner: Vec<u8>,
}
impl BorshSerialize for Script {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        for b in &self.inner {
            b.serialize(writer)?;
        }
        Ok(())
    }
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
