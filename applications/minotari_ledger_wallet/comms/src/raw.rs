// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Untyped APDU builders: for payloads the typed encoders refuse to construct.
//!
//! Every instruction the wallet sends goes through the shared codec (`minotari_ledger_wallet_common::codec`), via
//! [`Command::from_request`], which can only build a well formed request. That is the point of it - and it is
//! exactly why this module has to exist as well. The device's validation is the one that counts, so it has to be
//! exercised with the requests a well behaved host would never send: a payload one byte short, a branch the shared
//! enum cannot name, a `GetScriptOffset` chunk sequence that skips a number or resumes after a rejection.
//! `ledger_demo` and the Speculos scenario suite (`minotari_ledger_wallet_comms_testing`) send those through here.
//!
//! These are deliberately **not** sealed or feature gated. A malformed APDU is nothing a device does not already have
//! to survive from any host at all, so there is nothing to protect by hiding the ability to send one - and hiding
//! it would leave the device's rejection paths testable only by re-implementing the transport.
//!
//! What these do still take from the codec is the class byte and the account prefix, so a probe differs from a real
//! request only in the part it means to.

use ledger_transport::APDUCommand;
use minotari_ledger_wallet_common::{codec::CLA, common_types::Instruction};

use crate::ledger_wallet::Command;

/// A single exchange instruction with an arbitrary payload after the account.
pub fn build_command(account: u64, instruction: Instruction, data: Vec<u8>) -> Command<Vec<u8>> {
    let mut base_data = account.to_le_bytes().to_vec();
    base_data.extend_from_slice(&data);

    Command::new(APDUCommand {
        cla: CLA,
        ins: instruction.as_byte(),
        p1: 0x00,
        p2: 0x00,
        data: base_data,
    })
}

/// A single chunk of a chunked instruction with an explicit chunk number and continuation flag.
///
/// [`Command::chunk_command`] always emits a well formed 0, 1, 2, ... sequence. This builds one arbitrary chunk, so
/// that `ledger_demo` can drive the device's own validation on real hardware - including the malformed sequences
/// the accessor methods refuse to send, which are exactly the ones the device has to reject.
pub fn build_chunk_command(
    account: u64,
    instruction: Instruction,
    chunk_number: u8,
    more: bool,
    chunk: Vec<u8>,
) -> Command<Vec<u8>> {
    // The account is only carried on the first chunk, matching `chunk_command`.
    let mut base_data = vec![];
    if chunk_number == 0 {
        base_data.extend_from_slice(&account.to_le_bytes());
    }
    base_data.extend_from_slice(&chunk);

    Command::new(APDUCommand {
        cla: CLA,
        ins: instruction.as_byte(),
        p1: chunk_number,
        p2: u8::from(more),
        data: base_data,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    /// The APDU header and payload layout, pinned against accidental change. These are the bytes the device parses.
    #[test]
    fn apdu_layout_is_stable() {
        let command = build_command(1, Instruction::GetVersion, vec![0xaa]).to_apdu_command();
        assert_eq!(command.cla, CLA);
        assert_eq!(command.ins, Instruction::GetVersion.as_byte());
        assert_eq!(command.p1, 0x00);
        assert_eq!(command.p2, 0x00);
        // account (8 bytes, little endian) then the payload
        assert_eq!(command.data, vec![1, 0, 0, 0, 0, 0, 0, 0, 0xaa]);

        // The account rides on the first chunk only, and `p2` is the "more chunks follow" flag.
        let first = build_chunk_command(1, Instruction::GetScriptOffset, 0, true, vec![0xaa]).to_apdu_command();
        assert_eq!(
            (first.p1, first.p2, first.data),
            (0, 1, vec![1, 0, 0, 0, 0, 0, 0, 0, 0xaa])
        );
        let later = build_chunk_command(1, Instruction::GetScriptOffset, 3, false, vec![0xbb]).to_apdu_command();
        assert_eq!((later.p1, later.p2, later.data), (3, 0, vec![0xbb]));
    }
}
