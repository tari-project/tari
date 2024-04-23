// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;

use ledger_device_sdk::{ecc::make_bip32_path, io::Comm, ui::gadgets::SingleMessage};
use tari_crypto::ristretto::{
    pedersen::extended_commitment_factory::ExtendedPedersenCommitmentFactory,
    RistrettoComAndPubSig,
};

use crate::{
    alloc::string::ToString,
    utils::{get_key_from_canonical_bytes, get_key_from_uniform_bytes, get_raw_key, u64_to_string},
    AppSW,
    BIP32_COIN_TYPE,
    RESPONSE_VERSION,
};

const STATIC_INDEX: &str = "42";

const MAX_TRANSACTION_LEN: usize = 272;
pub struct SignerCtx {
    payload: [u8; MAX_TRANSACTION_LEN],
    payload_len: usize,
}

// Implement constructor for TxInfo with default values
impl SignerCtx {
    pub fn new() -> Self {
        Self {
            payload: [0u8; MAX_TRANSACTION_LEN],
            payload_len: 0,
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.payload = [0u8; MAX_TRANSACTION_LEN];
        self.payload_len = 0;
    }
}

pub fn handler_get_script_signature(
    comm: &mut Comm,
    chunk: u8,
    more: bool,
    signer_ctx: &mut SignerCtx,
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

    // Derive private key
    let mut account_bytes = [0u8; 8];
    account_bytes.clone_from_slice(&signer_ctx.payload[0..8]);
    let account = u64_to_string(u64::from_le_bytes(account_bytes));

    let mut bip32_path = "m/44'/".to_string();
    bip32_path.push_str(&BIP32_COIN_TYPE.to_string());
    bip32_path.push_str(&"'/");
    bip32_path.push_str(&account);
    bip32_path.push_str(&"'/0/");
    bip32_path.push_str(STATIC_INDEX);
    let path: [u32; 5] = make_bip32_path(bip32_path.as_bytes());

    SingleMessage::new(&bip32_path).show_and_wait();

    let raw_key = match get_raw_key(&path) {
        Ok(val) => val,
        Err(e) => {
            SingleMessage::new(&format!("Key error {:?}", e)).show_and_wait();
            return Err(AppSW::KeyDeriveFail);
        },
    };

    let script_private_key = get_key_from_uniform_bytes(&raw_key.as_ref())?;
    let value = get_key_from_canonical_bytes(&signer_ctx.payload[8..40])?;
    let spend_private_key = get_key_from_canonical_bytes(&signer_ctx.payload[48..80])?;
    let r_a = get_key_from_canonical_bytes(&signer_ctx.payload[88..120])?;
    let r_x = get_key_from_canonical_bytes(&signer_ctx.payload[128..160])?;
    let r_y = get_key_from_canonical_bytes(&signer_ctx.payload[168..200])?;
    let challenge = &signer_ctx.payload[208..272];

    let factory = ExtendedPedersenCommitmentFactory::default();

    SingleMessage::new(&"Signing...".to_string()).show();

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

    SingleMessage::new(&"Success!".to_string()).show_and_wait();

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&script_signature.to_vec());
    comm.reply_ok();

    Ok(())
}
