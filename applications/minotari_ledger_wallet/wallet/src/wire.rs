// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The device's end of the shared wire-format codec (`minotari_ledger_wallet_common::codec`).
//!
//! The codec owns every layout; this is only the glue between it and the SDK's `Comm`: a [`Writer`] over the reply
//! buffer, and the one place a framing failure is turned into a status word.

use ledger_device_sdk::io::Comm;
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::NbglStatus;
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::ui::gadgets::SingleMessage;
use minotari_ledger_wallet_common::codec::{ComAndPubSigReply, Encode, Writer};

use crate::{crypto::commitment_and_public_key_signature::CommitmentAndPublicKeySignature, AppSW};

/// Writes straight into the APDU reply buffer, so a reply is never assembled a second time on the stack or heap.
///
/// A newtype rather than an `impl Writer for Comm` only because both the trait and `Comm` are foreign here.
struct CommWriter<'a>(&'a mut Comm);

impl Writer for CommWriter<'_> {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.0.append(bytes);
    }
}

/// Append an encoded reply to the APDU response.
pub fn reply(comm: &mut Comm, reply: &impl Encode) {
    reply.encode(&mut CommWriter(comm));
}

/// Reply with a commitment and public key signature: both script signature instructions and
/// `GetOneSidedMetadataSignature` answer with one.
pub fn reply_com_and_pub_sig(comm: &mut Comm, signature: &CommitmentAndPublicKeySignature) {
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

/// A request whose payload was not a length its layout can have.
///
/// Tells the user and answers `WrongApduLength`, exactly as each handler's own length check did before the codec -
/// kept in one function now rather than copied into every handler, which is also smaller.
#[inline(never)]
pub fn invalid_data_length() -> AppSW {
    #[cfg(not(any(target_os = "stax", target_os = "flex")))]
    {
        SingleMessage::new("Invalid data length").show_and_wait();
    }

    #[cfg(any(target_os = "stax", target_os = "flex"))]
    {
        NbglStatus::new().text(&"Invalid data length").show(false);
    }
    AppSW::WrongApduLength
}
