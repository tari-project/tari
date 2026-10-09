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
    codec::{Decode, MetadataSignatureHeadChunk, PREIMAGE_CHUNK_SIZE},
    common_types::LedgerKeyBranch,
    get_payment_id_bytes_from_tari_dual_address,
    get_public_spend_key_bytes_from_tari_dual_address,
    get_public_view_key_bytes_from_tari_dual_address,
    metadata_output::{
        output_type_name,
        parse_metadata_preimage,
        MetadataPreimageError,
        MAX_METADATA_PREIMAGE_SIZE,
        OUTPUT_TYPE_STANDARD,
    },
    script_offset::is_pre_mine_sender_offset_index,
    tari_dual_address_display,
    u64_to_string,
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
    STATIC_VIEW_INDEX,
};

/// What `GetOneSidedMetadataSignature` has accumulated across its chunks.
///
/// Long lived, owned by the main loop like `ScriptOffsetCtx`, and reset by any other instruction and by any error, so
/// a rejected or interleaved exchange cannot be resumed. Everything in here is an owned copy taken from the APDU
/// buffer as each chunk arrived - nothing the review below shows, or the signature below commits to, is read from
/// the APDU buffer after a screen. See `wire`.
pub struct MetadataSignatureCtx {
    head: Option<Head>,
    preimage: Vec<u8>,
    next_chunk: u8,
}

/// Chunk 0, validated and copied.
struct Head {
    account: u64,
    network: u8,
    sender_offset_key_index: u64,
    sender_offset_key_type: KeyType,
    value: u64,
    commitment_mask: RistrettoSecretKey,
    receiver_address: Vec<u8>,
    preimage_size: usize,
}

impl MetadataSignatureCtx {
    pub fn new() -> Self {
        Self {
            head: None,
            preimage: Vec::new(),
            next_chunk: 0,
        }
    }

    /// Drop everything accumulated, and give the preimage's memory back.
    pub fn reset(&mut self) {
        self.head = None;
        self.preimage = Vec::new();
        self.next_chunk = 0;
    }
}

/// `GetOneSidedMetadataSignature`: chunk 0 is the head, the chunks after it the metadata signature preimage. See
/// `minotari_ledger_wallet_common::codec::metadata` for the layout and `minotari_ledger_wallet_common::metadata_output`
/// for what the preimage may hold.
///
/// Every chunk but the last is answered with an empty `Ok`. The last is answered with the signature, after the review
/// - or with none, for change to this wallet's own address with default features.
pub fn handler_get_one_sided_metadata_signature(
    comm: &mut Comm,
    chunk_number: u8,
    more: bool,
    ctx: &mut MetadataSignatureCtx,
) -> Result<(), AppSW> {
    if chunk_number == 0 {
        ctx.reset();
        // The head is never the last chunk: a preimage always follows.
        if !more {
            return Err(AppSW::WrongP1P2);
        }
        let head = read_head(comm)?;
        // Reserved up front, and fallibly: the size is bounded by `MAX_METADATA_PREIMAGE_SIZE`, but a heap that
        // cannot hold it must refuse rather than abort the application.
        ctx.preimage
            .try_reserve_exact(head.preimage_size)
            .map_err(|_| AppSW::MetadataSignatureFail)?;
        ctx.head = Some(head);
        ctx.next_chunk = 1;
        return Ok(());
    }

    let preimage_size = match &ctx.head {
        Some(head) => head.preimage_size,
        None => return Err(AppSW::WrongP1P2),
    };
    if chunk_number != ctx.next_chunk {
        return Err(AppSW::WrongP1P2);
    }
    {
        let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
        // Every chunk but the last is exactly `PREIMAGE_CHUNK_SIZE`, which bounds the number of chunks; the last
        // carries what is left, and no chunk may run past the size the head declared.
        let size_ok = if more {
            data.len() == PREIMAGE_CHUNK_SIZE
        } else {
            !data.is_empty() && data.len() <= PREIMAGE_CHUNK_SIZE
        };
        let total = ctx.preimage.len().saturating_add(data.len());
        if !size_ok || total > preimage_size || (more && total >= preimage_size) || (!more && total != preimage_size) {
            return Err(AppSW::WrongApduLength);
        }
        ctx.preimage.extend_from_slice(data);
    }
    ctx.next_chunk = ctx.next_chunk.checked_add(1).ok_or(AppSW::WrongP1P2)?;
    if more {
        return Ok(());
    }

    let head = ctx.head.take().ok_or(AppSW::WrongP1P2)?;
    let preimage = core::mem::take(&mut ctx.preimage);
    ctx.reset();
    sign(comm, head, preimage)
}

/// Read, validate and copy chunk 0.
fn read_head(comm: &mut Comm) -> Result<Head, AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    let head = MetadataSignatureHeadChunk::decode(data).map_err(|_| AppSW::WrongApduLength)?;

    // A `u64` on the wire but a single byte in the hash label: reject rather than truncate.
    let network = u8::try_from(head.network).map_err(|_| AppSW::WrongApduLength)?;
    let preimage_size = usize::from(head.preimage_size);
    if preimage_size == 0 || preimage_size > MAX_METADATA_PREIMAGE_SIZE {
        return Err(AppSW::WrongApduLength);
    }
    let commitment_mask: RistrettoSecretKey =
        get_key_from_canonical_bytes::<RistrettoSecretKey>(head.commitment_mask)?.into();

    // An ordinary one-sided output's sender offset key is on `OneSidedSenderOffset`; the backup pre-mine spend's is on
    // `PreMine`, because `GetScriptOffset` issues it in pre-mine mode, which always sets the pre-mine sender offset
    // marker - a `PreMine` index without it is a script key. Nothing else is a sender offset key. Signing either here
    // is safe: the nonces are drawn on the device, so there is no second signature under one nonce to difference
    // against.
    let sender_offset_key_index = head.sender_offset_key_index;
    let sender_offset_key_type = match branch_key_from_u64(head.sender_offset_branch)? {
        LedgerKeyBranch::OneSidedSenderOffset => KeyType::from_branch_key(head.sender_offset_branch)?,
        LedgerKeyBranch::PreMine if is_pre_mine_sender_offset_index(sender_offset_key_index) => {
            KeyType::from_branch_key(head.sender_offset_branch)?
        },
        LedgerKeyBranch::PreMine | LedgerKeyBranch::Random | LedgerKeyBranch::Spend => {
            return Err(AppSW::BadBranchKey)
        },
    };

    // Copied to the heap: up to `TARI_DUAL_ADDRESS_MAX_SIZE` (323) bytes is a lot of a Ledger stack. It is the same
    // owned copy the review shows and the signature's script is built from.
    let receiver_address = head.receiver_address().to_vec();
    if let Err(e) = tari_dual_address_display(&receiver_address) {
        show_error(&format!("Error: {:?}", e.to_string()));
        return Err(AppSW::MetadataSignatureFail);
    }

    Ok(Head {
        account: head.account,
        network,
        sender_offset_key_index,
        sender_offset_key_type,
        value: head.value,
        commitment_mask,
        receiver_address,
        preimage_size,
    })
}

/// Raw bytes, written to the hasher as they are, with no length prefix: the preimage already is the consensus
/// encoding of the fields `common` is the hash of.
struct Raw<'a>(&'a [u8]);

impl BorshSerialize for Raw<'_> {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(self.0)
    }
}

/// Parse the preimage, decide whether it needs a review, show it if it does, and sign.
fn sign(comm: &mut Comm, head: Head, preimage: Vec<u8>) -> Result<(), AppSW> {
    // What the device will not sign is refused before anything is shown. See
    // `minotari_ledger_wallet_common::metadata_output` for why the whole preimage is parsed: the fields are read at
    // the offsets a consensus decoder reads them from, so what is shown and decided on is what consensus will hold
    // the output to.
    let output = parse_metadata_preimage(&preimage).map_err(|e| match e {
        MetadataPreimageError::Malformed => AppSW::MetadataSignatureFail,
        MetadataPreimageError::CovenantNotEmpty | MetadataPreimageError::Burn | MetadataPreimageError::Coinbase => {
            AppSW::OutputNotSignable
        },
    })?;
    // `common`, from the same bytes that were just parsed. A host that sent fields other than the output's gets a
    // signature that verifies against no output carrying the fields it published.
    let common: [u8; 32] =
        DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message", head.network)
            .chain(&Raw(&preimage))
            .finalize()
            .into();
    drop(preimage);

    let receiver_address = tari_dual_address_display(&head.receiver_address).map_err(|_| AppSW::MetadataSignatureFail)?;
    let payment_id_bytes =
        get_payment_id_bytes_from_tari_dual_address(&head.receiver_address).map_err(|_| AppSW::MetadataSignatureFail)?;

    // Signed without a review only when the output is change in every respect the device can see: to this wallet's
    // own address - its spend key this device's own public `alpha` and its view key this device's own public view key,
    // for the account - with default features (a standard output, maturity 0, no coinbase data, no sidechain feature,
    // a bullet proof range proof), an empty covenant and no minimum value promise. Anything else is reviewed, and the
    // review shows what differs from that.
    //
    // Both keys are needed. The spend key binds the script - the device builds the standard stealth script for it -
    // but the host derives the commitment mask and encrypted data from the view key, so an address with this wallet's
    // spend key and someone else's view key gives an output locked to `alpha` that this wallet's scanner never finds.
    //
    // Both comparisons are against keys this device derives, never keys the host supplied, and both address keys are
    // read from the owned copy the signature's script is built from. Each key is derived, compared and dropped in its
    // own block, to keep the stack small, and only when everything before it matched.
    let may_skip_review = output.may_skip_review();
    let is_own_spend_key = may_skip_review && {
        let own_public_alpha = RistrettoPublicKey::from_secret_key(&derive_from_bip32_key(
            head.account,
            STATIC_SPEND_INDEX,
            KeyType::Spend,
        )?);
        match get_public_spend_key_bytes_from_tari_dual_address(&head.receiver_address) {
            Ok(bytes) => &bytes == own_public_alpha.as_array(),
            Err(_) => false,
        }
    };
    let is_change_to_self = is_own_spend_key && {
        let own_public_view_key = RistrettoPublicKey::from_secret_key(&derive_from_bip32_key(
            head.account,
            STATIC_VIEW_INDEX,
            KeyType::ViewKey,
        )?);
        match get_public_view_key_bytes_from_tari_dual_address(&head.receiver_address) {
            Ok(bytes) => &bytes == own_public_view_key.as_array(),
            Err(_) => false,
        }
    };

    if !is_change_to_self {
        let amount = Minotari::new(head.value).to_string();
        let payment_id = if payment_id_bytes.is_empty() {
            None
        } else {
            Some(format!("{} bytes", payment_id_bytes.len()))
        };
        let output_type = if output.output_type == OUTPUT_TYPE_STANDARD {
            None
        } else {
            Some(output_type_name(output.output_type).to_string())
        };
        let maturity = if output.maturity == 0 {
            None
        } else {
            Some(u64_to_string(output.maturity))
        };
        let sidechain = output.sidechain.map(|kind| kind.name().to_string());
        let validator_node = output.validator_node_public_key.as_ref().map(hex);
        let minimum_value_promise = if output.minimum_value_promise == 0 {
            None
        } else {
            Some(Minotari::new(output.minimum_value_promise).to_string())
        };
        let range_proof = if output.range_proof_type == 0 {
            None
        } else {
            Some("Revealed value".to_string())
        };

        let mut fields = Vec::new();
        fields.push(Field {
            name: "Amount",
            value: amount.as_str(),
        });
        fields.push(Field {
            name: "Receiver",
            value: receiver_address.as_str(),
        });
        if let Some(value) = &payment_id {
            fields.push(Field {
                name: "Payment ID",
                value: value.as_str(),
            });
        }
        if let Some(value) = &output_type {
            fields.push(Field {
                name: "Output type",
                value: value.as_str(),
            });
        }
        if let Some(value) = &maturity {
            fields.push(Field {
                name: "Maturity",
                value: value.as_str(),
            });
        }
        if let Some(value) = &sidechain {
            fields.push(Field {
                name: "Sidechain",
                value: value.as_str(),
            });
        }
        if let Some(value) = &validator_node {
            fields.push(Field {
                name: "Validator node",
                value: value.as_str(),
            });
        }
        if let Some(value) = &minimum_value_promise {
            fields.push(Field {
                name: "Min value",
                value: value.as_str(),
            });
        }
        if let Some(value) = &range_proof {
            fields.push(Field {
                name: "Range proof",
                value: value.as_str(),
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
            if !with_screen(comm, || review.show(fields_array)) {
                return Err(AppSW::UserCancelled);
            }
        }
    }

    let value_as_private_key: RistrettoSecretKey = head.value.into();

    let sender_offset_private_key =
        derive_from_bip32_key(head.account, head.sender_offset_key_index, head.sender_offset_key_type)?;
    let sender_offset_public_key = RistrettoPublicKey::from_secret_key(&sender_offset_private_key);

    let r_a = get_random_nonce()?;
    let r_x = get_random_nonce()?;
    let ephemeral_private_key = get_random_nonce()?;

    let factory = PedersenCommitmentFactory::default();

    let commitment = factory.commit(&head.commitment_mask, &value_as_private_key);
    let ephemeral_commitment = factory.commit(&r_x, &r_a);
    let ephemeral_pubkey = RistrettoPublicKey::from_secret_key(&ephemeral_private_key);

    let receiver_public_spend_key: RistrettoPublicKey =
        match get_public_spend_key_bytes_from_tari_dual_address(&head.receiver_address) {
            Ok(bytes) => get_key_from_canonical_bytes::<RistrettoPublicKey>(&bytes)?,
            Err(e) => {
                show_error(&format!("Error: {:?}", e.to_string()));
                return Err(AppSW::MetadataSignatureFail);
            },
        };

    let script = tari_script_with_address(&head.commitment_mask, &receiver_public_spend_key)?;
    let metadata_signature_message = metadata_signature_message_from_script_and_common(head.network, &script, &common);

    let challenge = finalize_metadata_signature_challenge(
        head.network,
        &sender_offset_public_key,
        &ephemeral_commitment,
        &ephemeral_pubkey,
        &commitment,
        &metadata_signature_message,
    );

    let metadata_signature = match CommitmentAndPublicKeySignature::sign(
        &value_as_private_key,
        &head.commitment_mask,
        &sender_offset_private_key,
        &r_a,
        &r_x,
        &ephemeral_private_key,
        &challenge,
        &factory,
    ) {
        Ok(sig) => sig,
        Err(_e) => {
            show_error("Signing error: Invalid challenge");
            return Err(AppSW::MetadataSignatureFail);
        },
    };

    reply_com_and_pub_sig(comm, &metadata_signature);

    Ok(())
}

/// Show an error and wait, on either toolkit.
fn show_error(message: &str) {
    #[cfg(not(any(target_os = "stax", target_os = "flex")))]
    {
        SingleMessage::new(message).show_and_wait();
    }
    #[cfg(any(target_os = "stax", target_os = "flex"))]
    {
        NbglStatus::new().text(message).show(false);
    }
}

/// Lower case hex, for a 32 byte key on the review screen.
fn hex(bytes: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0x0f] {
            if let Some(&digit) = DIGITS.get(usize::from(nibble)) {
                out.push(char::from(digit));
            }
        }
    }
    out
}

fn finalize_metadata_signature_challenge(
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
