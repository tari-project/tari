// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::{
    ecc::{Secp256k1, SeedDerive},
    hash::{sha3::Keccak256, HashInit},
    io::Comm,
};
use serde::Deserialize;
use serde_json_core::from_slice;

use crate::{app_ui::sign::ui_display_tx, utils::Bip32Path, AppSW};

const MAX_TRANSACTION_LEN: usize = 510;

#[derive(Deserialize)]
pub struct Tx<'a> {
    #[allow(dead_code)]
    nonce: u64,
    pub coin: &'a str,
    pub value: u64,
    #[serde(with = "hex::serde")] // Allows JSON deserialization from hex string
    pub to: [u8; 20],
    pub memo: &'a str,
}

pub struct TxContext {
    raw_tx: [u8; MAX_TRANSACTION_LEN], // raw transaction serialized
    raw_tx_len: usize,                 // length of raw transaction
    path: Bip32Path,
}

// Implement constructor for TxInfo with default values
impl TxContext {
    pub fn new() -> TxContext {
        TxContext {
            raw_tx: [0u8; MAX_TRANSACTION_LEN],
            raw_tx_len: 0,
            path: Default::default(),
        }
    }

    // Implement reset for TxInfo
    fn reset(&mut self) {
        self.raw_tx = [0u8; MAX_TRANSACTION_LEN];
        self.raw_tx_len = 0;
        self.path = Default::default();
    }
}

pub fn handler_sign_tx(comm: &mut Comm, chunk: u8, more: bool, ctx: &mut TxContext) -> Result<(), AppSW> {
    // Try to get data from comm
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    // First chunk, try to parse the path
    if chunk == 0 {
        // Reset transaction context
        ctx.reset();
        // This will propagate the error if the path is invalid
        ctx.path = data.try_into()?;
        Ok(())
    // Next chunks, append data to raw_tx and return or parse
    // the transaction if it is the last chunk.
    } else {
        if ctx.raw_tx_len + data.len() > MAX_TRANSACTION_LEN {
            return Err(AppSW::TxWrongLength);
        }

        // Append data to raw_tx
        ctx.raw_tx[ctx.raw_tx_len..ctx.raw_tx_len + data.len()].copy_from_slice(data);
        ctx.raw_tx_len += data.len();

        // If we expect more chunks, return
        if more {
            Ok(())
        // Otherwise, try to parse the transaction
        } else {
            // Try to deserialize the transaction
            let (tx, _): (Tx, usize) = from_slice(&ctx.raw_tx[..ctx.raw_tx_len]).map_err(|_| AppSW::TxParsingFail)?;
            // Display transaction. If user approves
            // the transaction, sign it. Otherwise,
            // return a "deny" status word.
            if ui_display_tx(&tx)? {
                compute_signature_and_append(comm, ctx)
            } else {
                Err(AppSW::Deny)
            }
        }
    }
}

fn compute_signature_and_append(comm: &mut Comm, ctx: &mut TxContext) -> Result<(), AppSW> {
    let mut keccak256 = Keccak256::new();
    let mut message_hash: [u8; 32] = [0u8; 32];

    let _ = keccak256.hash(&ctx.raw_tx[..ctx.raw_tx_len], &mut message_hash);

    let (sig, siglen, parity) = Secp256k1::derive_from_path(ctx.path.as_ref())
        .deterministic_sign(&message_hash)
        .map_err(|_| AppSW::TxSignFail)?;
    comm.append(&[siglen as u8]);
    comm.append(&sig[..siglen as usize]);
    comm.append(&[parity as u8]);
    Ok(())
}
