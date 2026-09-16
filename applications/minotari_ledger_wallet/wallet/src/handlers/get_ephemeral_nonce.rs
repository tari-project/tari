// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use ledger_device_sdk::io::Comm;
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::NbglStatus;
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::gadgets::SingleMessage;
use minotari_ledger_wallet_common::ephemeral_nonce::{EphemeralNonceStore, EphemeralNonceStoreError};
use tari_utilities::ByteArray;

use crate::{
    crypto::keys::{RistrettoPublicKey, RistrettoSecretKey},
    utils::get_random_nonce,
    AppSW,
    RESPONSE_VERSION,
};

/// The device's live ephemeral nonces.
///
/// This lives in RAM and is owned by the event loop, so leaving the application or power cycling the device drops
/// every outstanding nonce. That is the intended lifetime: a nonce that survived a session would be one more thing
/// a host could come back and ask to reuse.
pub type EphemeralNonceCtx = EphemeralNonceStore<RistrettoSecretKey>;

pub fn nonce_store_error_to_app_sw(e: EphemeralNonceStoreError) -> AppSW {
    match e {
        EphemeralNonceStoreError::Full => AppSW::NonceStoreFull,
        EphemeralNonceStoreError::InvalidHandle => AppSW::NonceHandleInvalid,
    }
}

/// Draw a fresh nonce on the device and hand back only its handle and public form.
///
/// The private nonce is never emitted. A host that could choose or read a nonce could recover the private key of
/// anything signed with it, so the only thing it is trusted with is echoing back the handle it was given.
pub fn handler_generate_ephemeral_nonce(comm: &mut Comm, nonce_ctx: &mut EphemeralNonceCtx) -> Result<(), AppSW> {
    let data = comm.get_data().map_err(|_| AppSW::WrongApduLength)?;
    // The transport prepends the account to every command and nothing else is carried here: the nonce is a random
    // scalar, not a derived key, so there is no path for the host to influence.
    if data.len() != 8 {
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

    let private_nonce = get_random_nonce()?;
    let public_nonce = RistrettoPublicKey::from_secret_key(&private_nonce);
    let handle = nonce_ctx.insert(private_nonce).map_err(nonce_store_error_to_app_sw)?;

    comm.append(&[RESPONSE_VERSION]); // version
    comm.append(&handle.to_le_bytes());
    comm.append(public_nonce.as_bytes());

    Ok(())
}
