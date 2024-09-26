// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod hashing;
pub mod utils;

mod app_ui {
    pub mod menu;
}
mod handlers {
    pub mod get_dh_shared_secret;
    pub mod get_one_sided_metadata_signature;
    pub mod get_public_key;
    pub mod get_public_spend_key;
    pub mod get_schnorr_signature;
    pub mod get_script_offset;
    pub mod get_script_signature;
    pub mod get_version;
    pub mod get_view_key;
}

use app_ui::menu::ui_menu_main;
use handlers::{
    get_dh_shared_secret::handler_get_dh_shared_secret,
    get_one_sided_metadata_signature::handler_get_one_sided_metadata_signature,
    get_public_key::handler_get_public_key,
    get_public_spend_key::handler_get_public_spend_key,
    get_schnorr_signature::{handler_get_raw_schnorr_signature, handler_get_script_schnorr_signature},
    get_script_offset::{handler_get_script_offset, ScriptOffsetCtx},
    get_script_signature::{handler_get_script_signature_derived, handler_get_script_signature_managed},
    get_version::handler_get_version,
    get_view_key::handler_get_view_key,
};
#[cfg(not(any(target_os = "stax", target_os = "flex")))]
use ledger_device_sdk::io::Event;
use ledger_device_sdk::io::{ApduHeader, Comm, Reply, StatusWords};
#[cfg(any(target_os = "stax", target_os = "flex"))]
use ledger_device_sdk::nbgl::{init_comm, NbglReviewStatus, StatusType};
#[cfg(feature = "pending_review_screen")]
use ledger_device_sdk::ui::gadgets::display_pending_review;
use minotari_ledger_wallet_common::common_types::{
    AppSW as AppSWMapping,
    Branch as BranchMapping,
    Instruction as InstructionMapping,
};
ledger_device_sdk::set_panic!(ledger_device_sdk::exiting_panic);

static BIP32_COIN_TYPE: u32 = 535348;
static CLA: u8 = 0x80;
static RESPONSE_VERSION: u8 = 1;

// Application status words.
#[repr(u16)]
#[derive(Debug, Clone, PartialEq)]
pub enum AppSW {
    Deny = AppSWMapping::Deny as u16,
    WrongP1P2 = AppSWMapping::WrongP1P2 as u16,
    InsNotSupported = AppSWMapping::InsNotSupported as u16,
    ScriptSignatureFail = AppSWMapping::ScriptSignatureFail as u16,
    RawSchnorrSignatureFail = AppSWMapping::RawSchnorrSignatureFail as u16,
    SchnorrSignatureFail = AppSWMapping::SchnorrSignatureFail as u16,
    ScriptOffsetNotUnique = AppSWMapping::ScriptOffsetNotUnique as u16,
    KeyDeriveFail = AppSWMapping::KeyDeriveFail as u16,
    KeyDeriveFromCanonical = AppSWMapping::KeyDeriveFromCanonical as u16,
    KeyDeriveFromUniform = AppSWMapping::KeyDeriveFromUniform as u16,
    RandomNonceFail = AppSWMapping::RandomNonceFail as u16,
    BadBranchKey = AppSWMapping::BadBranchKey as u16,
    MetadataSignatureFail = AppSWMapping::MetadataSignatureFail as u16,
    WrongApduLength = StatusWords::BadLen as u16, // See ledger-device-rust-sdk/ledger_device_sdk/src/io.rs:16
    UserCancelled = StatusWords::UserCancelled as u16, // See ledger-device-rust-sdk/ledger_device_sdk/src/io.rs:16
    Ok = AppSWMapping::Ok as u16,
}

impl From<AppSW> for Reply {
    fn from(sw: AppSW) -> Reply {
        Reply(sw as u16)
    }
}

/// Possible input commands received through APDUs.
#[derive(Clone, Copy)]
pub enum Instruction {
    GetVersion,
    GetAppName,
    GetPublicKey,
    GetPublicSpendKey,
    GetScriptSignatureManaged,
    GetScriptSignatureDerived,
    GetScriptOffset { chunk_number: u8, more: bool },
    GetViewKey,
    GetDHSharedSecret,
    GetRawSchnorrSignature,
    GetScriptSchnorrSignature,
    GetOneSidedMetadataSignature,
}

const P2_MORE: u8 = 0x01;
const STATIC_SPEND_INDEX: u64 = 42;
const STATIC_VIEW_INDEX: u64 = 57311; // No significance, just a random number by large dice roll
const MAX_PAYLOADS: u8 = 250;

#[repr(u8)]
#[derive(Debug, PartialEq)]
pub enum KeyType {
    Spend = 0x01,
    Nonce = 0x02,
    ViewKey = 0x03,
    OneSidedSenderOffset = 0x04,
    Random = 0x06,
    PreMine = 0x07,
    MetadataEphemeralNonce = 0x08,
}

impl KeyType {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    fn from_branch_key(n: u64) -> Result<Self, AppSW> {
        if n > u64::from(u8::MAX) {
            return Err(AppSW::BadBranchKey);
        }
        if let Some(branch) = BranchMapping::from_byte(n as u8) {
            match branch {
                BranchMapping::OneSidedSenderOffset => Ok(Self::OneSidedSenderOffset),
                BranchMapping::Spend => Ok(Self::Spend),
                BranchMapping::RandomKey => Ok(Self::Random),
                BranchMapping::PreMine => Ok(Self::PreMine),
                BranchMapping::MetadataEphemeralNonce => Ok(Self::MetadataEphemeralNonce),
                _ => Err(AppSW::BadBranchKey),
            }
        } else {
            return Err(AppSW::BadBranchKey);
        }
    }
}

impl TryFrom<ApduHeader> for Instruction {
    type Error = AppSW;

    /// APDU parsing logic.
    ///
    /// Parses INS, P1 and P2 bytes to build an [`Instruction`]. P1 and P2 are translated to
    /// strongly typed variables depending on the APDU instruction code. Invalid INS, P1 or P2
    /// values result in errors with a status word, which are automatically sent to the host by the
    /// SDK.
    ///
    /// This design allows a clear separation of the APDU parsing logic and commands handling.
    ///
    /// Note that CLA is not checked here. Instead the method [`Comm::set_expected_cla`] is used in
    /// [`sample_main`] to have this verification automatically performed by the SDK.
    fn try_from(value: ApduHeader) -> Result<Self, Self::Error> {
        let ins = InstructionMapping::from_byte(value.ins).ok_or(AppSW::InsNotSupported)?;
        match (ins, value.p1, value.p2) {
            (InstructionMapping::GetVersion, 0, 0) => Ok(Instruction::GetVersion),
            (InstructionMapping::GetAppName, 0, 0) => Ok(Instruction::GetAppName),
            (InstructionMapping::GetPublicSpendKey, 0, 0) => Ok(Instruction::GetPublicSpendKey),
            (InstructionMapping::GetPublicKey, 0, 0) => Ok(Instruction::GetPublicKey),
            (InstructionMapping::GetScriptSignatureManaged, 0, 0) => Ok(Instruction::GetScriptSignatureManaged),
            (InstructionMapping::GetScriptSignatureDerived, 0, 0) => Ok(Instruction::GetScriptSignatureDerived),
            (InstructionMapping::GetScriptOffset, 0..=MAX_PAYLOADS, 0 | P2_MORE) => Ok(Instruction::GetScriptOffset {
                chunk_number: value.p1,
                more: value.p2 == P2_MORE,
            }),
            (InstructionMapping::GetViewKey, 0, 0) => Ok(Instruction::GetViewKey),
            (InstructionMapping::GetDHSharedSecret, 0, 0) => Ok(Instruction::GetDHSharedSecret),
            (InstructionMapping::GetRawSchnorrSignature, 0, 0) => Ok(Instruction::GetRawSchnorrSignature),
            (InstructionMapping::GetScriptSchnorrSignature, 0, 0) => Ok(Instruction::GetScriptSchnorrSignature),
            (InstructionMapping::GetOneSidedMetadataSignature, 0, 0) => Ok(Instruction::GetOneSidedMetadataSignature),
            (InstructionMapping::GetScriptSchnorrSignature, _, _) => Err(AppSW::WrongP1P2),
            (_, _, _) => Err(AppSW::InsNotSupported),
        }
    }
}

#[cfg(any(target_os = "stax", target_os = "flex"))]
fn show_status_and_home_if_needed(status: &AppSW) {
    // fn show_status_and_home_if_needed(ins: &Instruction, tx_ctx: &ScriptOffsetCtx, status: &AppSW) {
    // let (show_status, status_type) = match (ins, status) {
    //     (Instruction::GetPubkey { display: true }, AppSW::Deny | AppSW::Ok) => {
    //         (true, StatusType::Address)
    //     }
    //     (Instruction::SignTx { .. }, AppSW::Deny | AppSW::Ok) if tx_ctx.finished() => {
    //         (true, StatusType::Transaction)
    //     }
    //     (_, _) => (false, StatusType::Transaction),
    // };

    // if show_status {
    if true {
        let success = *status == AppSW::Ok;
        NbglReviewStatus::new()
            .status_type(StatusType::Transaction)
            .show(success);
    }
}

#[no_mangle]
extern "C" fn sample_main() {
    // Create the communication manager, and configure it to accept only APDU from the 0x80 class.
    // If any APDU with a wrong class value is received, comm will respond automatically with
    // BadCla status word.
    let mut comm = Comm::new().set_expected_cla(CLA);

    // This is long-lived over the span the ledger app is open, across multiple interactions
    let mut offset_ctx = ScriptOffsetCtx::new();
    #[cfg(any(target_os = "stax", target_os = "flex"))]
    {
        // Initialize reference to Comm instance for NBGL
        // API calls.
        init_comm(&mut comm);
        offset_ctx.home = ui_menu_main(&mut comm);
        offset_ctx.home.show_and_return();
    }

    loop {
        #[cfg(any(target_os = "stax", target_os = "flex"))]
        let ins: Instruction = comm.next_command();

        #[cfg(not(any(target_os = "stax", target_os = "flex")))]
        let ins = if let Event::Command(ins) = ui_menu_main(&mut comm) {
            ins
        } else {
            continue;
        };

        let _status = match handle_apdu(&mut comm, ins, &mut offset_ctx) {
            Ok(()) => {
                comm.reply_ok();
                AppSW::Ok
            },
            Err(sw) => {
                comm.reply(sw.clone());
                sw
            },
        };
        #[cfg(any(target_os = "stax", target_os = "flex"))]
        show_status_and_home_if_needed(&_status);
        // show_status_and_home_if_needed(&ins, &mut offset_ctx, &_status);
    }
}

fn handle_apdu(comm: &mut Comm, ins: Instruction, offset_ctx: &mut ScriptOffsetCtx) -> Result<(), AppSW> {
    match ins {
        Instruction::GetVersion => handler_get_version(comm),
        Instruction::GetAppName => {
            comm.append(env!("CARGO_PKG_NAME").as_bytes());
            Ok(())
        },
        Instruction::GetPublicKey => handler_get_public_key(comm),
        Instruction::GetPublicSpendKey => handler_get_public_spend_key(comm),
        Instruction::GetScriptSignatureManaged => handler_get_script_signature_managed(comm),
        Instruction::GetScriptSignatureDerived => handler_get_script_signature_derived(comm),
        Instruction::GetScriptOffset { chunk_number, more } => {
            handler_get_script_offset(comm, chunk_number, more, offset_ctx)
        },
        Instruction::GetViewKey => handler_get_view_key(comm),
        Instruction::GetDHSharedSecret => handler_get_dh_shared_secret(comm),
        Instruction::GetRawSchnorrSignature => handler_get_raw_schnorr_signature(comm),
        Instruction::GetScriptSchnorrSignature => handler_get_script_schnorr_signature(comm),
        Instruction::GetOneSidedMetadataSignature => handler_get_one_sided_metadata_signature(comm),
    }
}
