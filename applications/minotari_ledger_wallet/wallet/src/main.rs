// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod utils;

mod app_ui {
    pub mod menu;
}
mod handlers {
    pub mod get_public_key;
    pub mod get_script_offset;
    pub mod get_script_signature;
    pub mod get_version;
}

use core::mem::MaybeUninit;

use app_ui::menu::ui_menu_main;
use critical_section::RawRestoreState;
use handlers::{
    get_public_key::handler_get_public_key,
    get_script_offset::{handler_get_script_offset, ScriptOffsetCtx},
    get_script_signature::{handler_get_script_signature, ScriptSignatureCtx},
    get_version::handler_get_version,
};
#[cfg(feature = "pending_review_screen")]
use ledger_device_sdk::ui::gadgets::display_pending_review;
use ledger_device_sdk::{
    io::{ApduHeader, Comm, Event, Reply, StatusWords},
    ui::gadgets::SingleMessage,
};

ledger_device_sdk::set_panic!(ledger_device_sdk::exiting_panic);

static BIP32_COIN_TYPE: u32 = 535348;
static CLA: u8 = 0x80;

static RESPONSE_VERSION: u8 = 1;

/// Allocator heap size
const HEAP_SIZE: usize = 1024 * 26;

/// Statically allocated heap memory
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

/// Bind global allocator
#[global_allocator]
static HEAP: embedded_alloc::Heap = embedded_alloc::Heap::empty();

/// Error handler for allocation
#[alloc_error_handler]
fn alloc_error(_: core::alloc::Layout) -> ! {
    SingleMessage::new("allocation error!").show_and_wait();
    ledger_device_sdk::exit_app(250)
}

/// Initialise allocator
pub fn init() {
    unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
}

struct MyCriticalSection;
critical_section::set_impl!(MyCriticalSection);

unsafe impl critical_section::Impl for MyCriticalSection {
    unsafe fn acquire() -> RawRestoreState {
        // nothing, it's all good, don't worry bout it
    }

    unsafe fn release(_token: RawRestoreState) {
        // nothing, it's all good, don't worry bout it
    }
}

// Application status words.
#[repr(u16)]
pub enum AppSW {
    Deny = 0x6985,
    WrongP1P2 = 0x6A86,
    InsNotSupported = 0x6D00,
    ClaNotSupported = 0x6E00,
    ScriptSignatureFail = 0xB001,
    KeyDeriveFail = 0xB009,
    VersionParsingFail = 0xB00A,
    WrongApduLength = StatusWords::BadLen as u16,
}

impl From<AppSW> for Reply {
    fn from(sw: AppSW) -> Reply {
        Reply(sw as u16)
    }
}

/// Possible input commands received through APDUs.
pub enum Instruction {
    GetVersion,
    GetAppName,
    GetPublicKey,
    GetScriptSignature { chunk: u8, more: bool },
    GetScriptOffset { chunk: u8, more: bool },
}

const P2_MORE: u8 = 0x01;
const STATIC_ALPHA_INDEX: u64 = 42;

pub enum KeyType {
    Alpha,
    Nonce,
    Recovery,
    ScriptOffset,
}

impl KeyType {
    fn to_byte(&self) -> u8 {
        match self {
            Self::Alpha => 1,
            Self::Nonce => 2,
            Self::Recovery => 3,
            Self::ScriptOffset => 4,
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
        match (value.ins, value.p1, value.p2) {
            (1, 0, 0) => Ok(Instruction::GetVersion),
            (2, 0, 0) => Ok(Instruction::GetAppName),
            (4, 0, 0) => Ok(Instruction::GetPublicKey),
            (5, 0..=6, 0 | P2_MORE) => Ok(Instruction::GetScriptSignature {
                chunk: value.p1,
                more: value.p2 == P2_MORE,
            }),
            (6, 0..=6, 0 | P2_MORE) => Ok(Instruction::GetScriptOffset {
                chunk: value.p1,
                more: value.p2 == P2_MORE,
            }),
            (3..=4, _, _) => Err(AppSW::WrongP1P2),
            (_, _, _) => Err(AppSW::InsNotSupported),
        }
    }
}

#[no_mangle]
extern "C" fn sample_main() {
    init();
    // Create the communication manager, and configure it to accept only APDU from the 0x80 class.
    // If any APDU with a wrong class value is received, comm will respond automatically with
    // BadCla status word.
    let mut comm = Comm::new().set_expected_cla(CLA);

    // Developer mode / pending review popup
    // must be cleared with user interaction
    #[cfg(feature = "pending_review_screen")]
    display_pending_review(&mut comm);

    // This is long lived over the span the ledger app is open, across multiple interactions
    let mut signer_ctx = ScriptSignatureCtx::new();
    let mut offset_ctx = ScriptOffsetCtx::new();

    loop {
        // Wait for either a specific button push to exit the app
        // or an APDU command
        if let Event::Command(ins) = ui_menu_main(&mut comm) {
            match handle_apdu(&mut comm, ins, &mut signer_ctx, &mut offset_ctx) {
                Ok(()) => comm.reply_ok(),
                Err(sw) => comm.reply(sw),
            }
        }
    }
}

fn handle_apdu(
    comm: &mut Comm,
    ins: Instruction,
    signer_ctx: &mut ScriptSignatureCtx,
    offset_ctx: &mut ScriptOffsetCtx,
) -> Result<(), AppSW> {
    match ins {
        Instruction::GetVersion => handler_get_version(comm),
        Instruction::GetAppName => {
            comm.append(env!("CARGO_PKG_NAME").as_bytes());
            Ok(())
        },
        Instruction::GetPublicKey => handler_get_public_key(comm),
        Instruction::GetScriptSignature { chunk, more } => handler_get_script_signature(comm, chunk, more, signer_ctx),
        Instruction::GetScriptOffset { chunk, more } => handler_get_script_offset(comm, chunk, more, offset_ctx),
    }
}
