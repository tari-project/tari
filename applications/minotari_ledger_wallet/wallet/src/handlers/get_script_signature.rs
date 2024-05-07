// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::{format, str::from_utf8};

use ledger_device_sdk::{
    io::Comm,
    ui::gadgets::{MessageScroller, SingleMessage},
};
use tari_crypto::{
    keys::PublicKey,
    ristretto::{
        pedersen::extended_commitment_factory::ExtendedPedersenCommitmentFactory,
        RistrettoComAndPubSig,
        RistrettoPublicKey,
        RistrettoSecretKey,
    },
    tari_utilities::{hex::Hex, ByteArray},
};

use crate::{
    alloc::string::ToString,
    utils::{alpha_hasher, derive_from_bip32_key, get_key_from_canonical_bytes, special_hash, u64_to_string},
    AppSW,
    KeyType,
    RESPONSE_VERSION,
    STATIC_ALPHA_INDEX,
};

const MAX_TRANSACTION_LEN: usize = 312;
pub struct ScriptSignatureCtx {
    payload: [u8; MAX_TRANSACTION_LEN],
    payload_len: usize,
    account: u64,
}

// Implement constructor for TxInfo with default values
impl ScriptSignatureCtx {
    pub fn new() -> Self {
        Self {
            payload: [0u8; MAX_TRANSACTION_LEN],
            payload_len: 0,
            account: 0,
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.payload = [0u8; MAX_TRANSACTION_LEN];
        self.payload_len = 0;
        self.account = 0;
    }
}

pub fn handler_get_script_signature(
    comm: &mut Comm,
    chunk: u8,
    more: bool,
    signer_ctx: &mut ScriptSignatureCtx,
) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;

    if chunk == 0 {
        // Reset transaction context
        signer_ctx.reset();
    }

    if signer_ctx.payload_len + data.len() > MAX_TRANSACTION_LEN {
        return Err(AppSW::ScriptSignatureFail);
    }

    // Append data to signer_ctx
    signer_ctx.payload[signer_ctx.payload_len..signer_ctx.payload_len + data.len()].copy_from_slice(data);
    signer_ctx.payload_len += data.len();

    // If we expect more chunks, return
    if more {
        return Ok(());
    }

    // Set the account for the transaction
    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&signer_ctx.payload[0..8]);
    signer_ctx.account = u64::from_le_bytes(account_bytes);
    MessageScroller::new(&u64_to_string(signer_ctx.account)).event_loop();

    let alpha = derive_from_bip32_key(signer_ctx.account, STATIC_ALPHA_INDEX, KeyType::Alpha)?;
    let blinding_factor: RistrettoSecretKey = get_key_from_canonical_bytes(&signer_ctx.payload[8..40])?;
    let alpha_pk = RistrettoPublicKey::from_secret_key(&alpha);
    let script_private_key = alpha_hasher(alpha, blinding_factor)?;

    let pk = RistrettoPublicKey::from_secret_key(&script_private_key);

    MessageScroller::new(&alpha_pk.to_string()).event_loop();
    MessageScroller::new(&pk.to_string()).event_loop();
    MessageScroller::new(&(&pk + alpha_pk).to_string()).event_loop();
    special_hash();

    let value: RistrettoSecretKey = get_key_from_canonical_bytes(&signer_ctx.payload[40..72])?;
    let spend_private_key: RistrettoSecretKey = get_key_from_canonical_bytes(&signer_ctx.payload[72..104])?;
    let r_a: RistrettoSecretKey = get_key_from_canonical_bytes(&signer_ctx.payload[104..136])?;
    let r_x: RistrettoSecretKey = get_key_from_canonical_bytes(&signer_ctx.payload[136..168])?;
    let r_y: RistrettoSecretKey = get_key_from_canonical_bytes(&signer_ctx.payload[168..200])?;
    let challenge = &signer_ctx.payload[200..264];

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
    signer_ctx.reset();
    comm.reply_ok();

    Ok(())
}
