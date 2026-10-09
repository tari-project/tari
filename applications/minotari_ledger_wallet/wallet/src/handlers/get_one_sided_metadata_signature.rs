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
    codec::{Decode, OneSidedMetadataSignatureHead},
    common_types::LedgerKeyBranch,
    get_payment_id_bytes_from_tari_dual_address,
    get_public_spend_key_bytes_from_tari_dual_address,
    script_offset::is_pre_mine_sender_offset_index,
    tari_dual_address_display,
};
use tari_utilities::ByteArray;
use zeroize::Zeroizing;

use crate::{
    alloc::string::ToString,
    crypto::{
        commitment::PedersenCommitment,
        commitment_and_public_key_signature::CommitmentAndPublicKeySignature,
        commitment_factory::PedersenCommitmentFactory,
        hashing::DomainSeparatedHasher,
        keys::{RistrettoPublicKey, RistrettoSecretKey},
    },
    hashing::DomainSeparatedConsensusHasher,
    utils::{
        derive_from_bip32_key,
        get_key_from_canonical_bytes,
        get_key_from_uniform_bytes,
        get_random_nonce,
        KeyManagerTransactionsHashDomain,
        TransactionHashDomain,
    },
    wire::{reply_com_and_pub_sig, with_screen},
    branch_key_from_u64,
    AppSW,
    KeyType,
    STATIC_SPEND_INDEX,
};

pub fn handler_get_one_sided_metadata_signature(comm: &mut Comm) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    // The layout has a variable length address in the middle, so the codec decodes it in stages that interleave with
    // the checks below exactly as the hand written parser did: the status word a malformed request gets depends on
    // which check fails first. See `minotari_ledger_wallet_common::codec::metadata`.
    let head = OneSidedMetadataSignatureHead::decode(data).map_err(|_| AppSW::WrongApduLength)?;

    let account = head.account;
    // A `u64` on the wire but a single byte in the hash label: reject rather than truncate.
    let network = u8::try_from(head.network).map_err(|_| AppSW::WrongApduLength)?;
    let txo_version = head.txo_version;
    let sender_offset_key_index = head.sender_offset_key_index;
    let value_u64 = head.value;
    let value = Minotari::new(head.value);

    let commitment_mask: RistrettoSecretKey =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(head.commitment_mask)?.into();

    let tail = head.receiver_address().map_err(|_| AppSW::WrongApduLength)?;

    // Everything read from here to the signature is an owned copy, never a borrow of `data`.
    //
    // `data` is the SDK's APDU buffer, and the review screen below does not leave it alone. On Stax and Flex,
    // `NbglReview::show` polls in `ux_sync_wait` -> `nbgl_next_event_ahead` -> `Comm::next_event_ahead` ->
    // `decode_event`, which copies *any* APDU that arrives while the screen is up into `apdu_buffer`
    // (`ledger_device_sdk` 1.35.0, `io_legacy.rs`: `self.apdu_buffer[0..272].copy_from_slice(..)`). So a borrow of
    // `data` read after the review reads whatever the host sent last, not what the user approved: a host could show
    // the user receiver A, send a second APDU carrying receiver B mid-review, and have the device sign a script
    // paying B once the user approves A.
    //
    // The whole address is copied, rather than only the spend key the signature needs, so that every check still
    // runs exactly where and in the order it always has - the address checksum before the review, the spend key's
    // canonical check after it - and a malformed request still gets the same status word from the same check. It is
    // copied to the heap, not the stack: up to `TARI_DUAL_ADDRESS_MAX_SIZE` (323) bytes is a lot of a Ledger stack,
    // and this handler already allocates for its review fields. The length was bounded by `receiver_address()` above,
    // so the copy itself cannot fail on a length.
    //
    // `wire::with_screen` below takes `&mut Comm` so that the borrow checker refuses to compile any use of `data`,
    // `head` or `tail` after the review.
    let receiver_address_bytes = tail.receiver_address.to_vec();

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

    // Copied for the same reason as the address: it is hashed into the signed message after the review.
    let metadata_signature_message_common: [u8; 32] = *tail.message().map_err(|_| AppSW::WrongApduLength)?;

    // The optional trailing `sender_offset_branch`, `OneSidedSenderOffset` when a host from before it existed leaves
    // it out - see `minotari_ledger_wallet_common::codec::metadata`. An ordinary one-sided output's sender offset key
    // is on `OneSidedSenderOffset`; the backup pre-mine spend's is on `PreMine`, because `GetScriptOffset` issues it
    // in pre-mine mode. Nothing else is a sender offset key, and the request is refused before the review. Signing
    // either branch here is safe: the nonces are drawn on the device, so there is no second signature under one
    // nonce to difference against.
    let sender_offset_branch = tail.sender_offset_branch().map_err(|_| AppSW::WrongApduLength)?;
    //
    // A `PreMine` sender offset only ever comes from `GetScriptOffset` in pre-mine mode, which always sets the pre-mine
    // sender offset marker; a `PreMine` index without it is a script key, and is refused here too.
    let sender_offset_key_type = match branch_key_from_u64(sender_offset_branch)? {
        LedgerKeyBranch::OneSidedSenderOffset => KeyType::from_branch_key(sender_offset_branch)?,
        LedgerKeyBranch::PreMine if is_pre_mine_sender_offset_index(sender_offset_key_index) => {
            KeyType::from_branch_key(sender_offset_branch)?
        },
        LedgerKeyBranch::PreMine | LedgerKeyBranch::Random | LedgerKeyBranch::Spend => {
            return Err(AppSW::BadBranchKey)
        },
    };

    // Extract payment ID if present
    let payment_id_bytes = get_payment_id_bytes_from_tari_dual_address(&receiver_address_bytes)
        .map_err(|_| AppSW::MetadataSignatureFail)?;

    // Change to this wallet is signed without a review. That is the case when the receiver's spend key is this
    // device's own public `alpha` for the account: the script below is always the standard stealth script for the
    // receiver's spend key - the device builds it itself from the address and the commitment mask, and the signature
    // commits to it - so the script is bound to this wallet's spend key. What is not inspected is everything in
    // `metadata_signature_message_common`, which reaches the device as an opaque hash: the output features, the
    // covenant, the encrypted data and the minimum value promise are host chosen. So "change" signed here with no
    // prompt can be a burn - which never runs its script - claimable on L2 by a key the host chooses, or be locked by
    // a long maturity or a covenant, or be unrecoverable from its encrypted data. That is no worse than before, when
    // change was signed raw with no screen at all; closing it needs these fields in the clear.
    //
    // The same rule auto-approves an explicit send to this wallet's own address, and a backup pre-mine spend whose
    // receiver is this device's own spend key.
    //
    // The comparison is against the `alpha` this device derives, never a key the host supplied, and the spend key is
    // read from the owned copy of the address, the same bytes the signature is built from after the review.
    let own_public_alpha =
        RistrettoPublicKey::from_secret_key(&derive_from_bip32_key(account, STATIC_SPEND_INDEX, KeyType::Spend)?);
    let is_change_to_self = match get_public_spend_key_bytes_from_tari_dual_address(&receiver_address_bytes) {
        Ok(bytes) => &bytes == own_public_alpha.as_array(),
        Err(_) => false,
    };

    if !is_change_to_self {
        let mut fields = Vec::new();
        let field_value = format!("{}", value.to_string());
        fields.push(Field {
            name: "Amount",
            value: &field_value,
        });
        let field_value = format!("{}", receiver_address);
        fields.push(Field {
            name: "Receiver",
            value: &field_value,
        });

        // Add payment ID field if present
        let payment_id_display = if !payment_id_bytes.is_empty() {
            format!("{} bytes", payment_id_bytes.len())
        } else {
            String::new()
        };

        if !payment_id_bytes.is_empty() {
            fields.push(Field {
                name: "Payment ID",
                value: &payment_id_display,
            });
        }

        let fields_array = fields.as_slice();

        #[cfg(not(any(target_os = "stax", target_os = "flex")))]
        {
            let review = MultiFieldReview::new(
                fields_array,
                &["Review ", "Transaction"],
                Some(&EYE),
                "Approve",
                Some(&VALIDATE_14),
                "Reject",
                Some(&CROSSMARK),
            );
            if !with_screen(comm, || review.show()) {
                return Err(AppSW::UserCancelled);
            }
        }
        #[cfg(any(target_os = "stax", target_os = "flex"))]
        {
            // Load glyph from 64x64 4bpp gif file with include_gif macro. Creates an NBGL compatible glyph.
            const TARI: NbglGlyph = NbglGlyph::from_include(include_gif!("key_64x64.gif", NBGL));
            // Create NBGL review. Maximum number of fields and string buffer length can be customised
            // with constant generic parameters of NbglReview. Default values are 32 and 1024 respectively.
            let review: NbglReview = NbglReview::new()
                .titles("Review transaction\nto send", "", "Sign transaction\nto send")
                .glyph(&TARI);

            //
            if !with_screen(comm, || review.show(fields_array)) {
                return Err(AppSW::UserCancelled);
            }
        }
    }

    let value_as_private_key: RistrettoSecretKey = value_u64.into();

    let sender_offset_private_key =
        derive_from_bip32_key(account, sender_offset_key_index, sender_offset_key_type)?;
    let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_private_key);

    let r_a = get_random_nonce()?;
    let r_x = get_random_nonce()?;
    let ephemeral_private_key = get_random_nonce()?;

    let factory = PedersenCommitmentFactory::default();

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

    let metadata_signature = match CommitmentAndPublicKeySignature::sign(
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
        Err(_e) => {
            let error_string = "Invalid challenge".to_string();

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
            return Err(AppSW::MetadataSignatureFail);
        },
    };

    reply_com_and_pub_sig(comm, &metadata_signature);

    Ok(())
}

fn finalize_metadata_signature_challenge(
    _version: u64,
    network: u8,
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

fn metadata_signature_message_from_script_and_common(network: u8, script: &Script, common: &[u8; 32]) -> [u8; 32] {
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
