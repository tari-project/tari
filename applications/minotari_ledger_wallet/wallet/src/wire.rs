// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The device's end of the shared wire-format codec (`minotari_ledger_wallet_common::codec`).
//!
//! The codec owns every layout; this is only the glue between it and the SDK's `Comm`: a [`Writer`] over the reply
//! buffer, the one place a framing failure is turned into a status word, and [`with_screen`].
//!
//! # Decoded requests borrow a buffer that UI screens overwrite
//!
//! Every decoded request borrows its fields from `comm.get_data()`, which is the SDK's APDU buffer - that is what
//! makes decoding zero copy. It is also why **no decoded field may be read after a UI screen has been shown**. On
//! Stax and Flex an NBGL screen polls for events while it is up (`ux_sync_wait` -> `nbgl_next_event_ahead` ->
//! `Comm::next_event_ahead` -> `decode_event`), and an APDU the host sends in the meantime is copied into that same
//! buffer (`ledger_device_sdk` 1.35.0, `io_legacy.rs`). A field read after the screen is then whatever the host sent
//! last, not what the user was shown. Copy anything needed after a screen into an owned value before it, and show
//! the screen through [`with_screen`], which makes the borrow checker enforce this.

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

/// Show a UI screen that waits for the user.
///
/// `comm` is not used. It is taken as `&mut` purely so that the borrow checker rejects any borrow of
/// `comm.get_data()` - a decoded request, or anything borrowed out of one - that is still live after the screen: the
/// SDK rewrites that buffer when an APDU arrives while the screen is up (see the module docs), so such a borrow would
/// read host bytes the user never reviewed. A handler that needs a request field after the screen must copy it first.
#[inline(always)]
pub fn with_screen<R>(_comm: &mut Comm, screen: impl FnOnce() -> R) -> R {
    screen()
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
