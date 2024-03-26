// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # MinoTari Ledger Wallet

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;
use core::{cmp::min, mem::MaybeUninit};

use critical_section::RawRestoreState;
use nanos_sdk::{
    buttons::ButtonEvent,
    io,
    io::{ApduHeader, Reply, StatusWords, SyscallError},
};
use nanos_ui::ui;
use tari_crypto::{ristretto::RistrettoSecretKey, tari_utilities::ByteArray};

use crate::{
    alloc::string::ToString,
    utils::{byte_to_hex, get_raw_key, u64_to_string},
};

static MINOTARI_LEDGER_ID: u32 = 535348;
static MINOTARI_ACCOUNT_ID: u32 = 7041;

pub mod hashing;
pub mod utils;

nanos_sdk::set_panic!(nanos_sdk::exiting_panic);

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
    ui::SingleMessage::new("allocation error!").show_and_wait();
    nanos_sdk::exit_app(250)
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

/// App Version parameters
const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

enum Instruction {
    GetVersion,
    GetPrivateKey,
    BadInstruction(u8),
    Exit,
}

impl From<io::ApduHeader> for Instruction {
    fn from(header: io::ApduHeader) -> Instruction {
        match header.ins {
            0x01 => Self::GetVersion,
            0x02 => Self::GetPrivateKey,
            0x03 => Self::Exit,
            other => Self::BadInstruction(other),
        }
    }
}

#[no_mangle]
extern "C" fn sample_main() {
    let mut comm = io::Comm::new();
    init();
    let messages = alloc::vec!["MinoTari Wallet", "keep the app open..", "[exit = both buttons]"];
    let mut index = 0;
    ui::SingleMessage::new(messages[index]).show();
    loop {
        let event = comm.next_event::<ApduHeader>();
        match event {
            io::Event::Button(ButtonEvent::BothButtonsRelease) => nanos_sdk::exit_app(0),
            io::Event::Button(ButtonEvent::RightButtonRelease) => {
                index = min(index + 1, messages.len() - 1);
                ui::SingleMessage::new(messages[index]).show()
            },
            io::Event::Button(ButtonEvent::LeftButtonRelease) => {
                if index > 0 {
                    index -= 1;
                }
                ui::SingleMessage::new(messages[index]).show()
            },
            io::Event::Button(_) => {},
            io::Event::Command(apdu_header) => match handle_apdu(&mut comm, apdu_header.into()) {
                Ok(()) => comm.reply_ok(),
                Err(e) => comm.reply(e),
            },
            io::Event::Ticker => {},
        }
    }
}

// Perform ledger instructions
fn handle_apdu(comm: &mut io::Comm, instruction: Instruction) -> Result<(), Reply> {
    if comm.rx == 0 {
        return Err(io::StatusWords::NothingReceived.into());
    }

    match instruction {
        Instruction::GetVersion => {
            ui::SingleMessage::new("GetVersion...").show();
            let name_bytes = NAME.as_bytes();
            let version_bytes = VERSION.as_bytes();
            comm.append(&[1]); // Format
            comm.append(&[name_bytes.len() as u8]);
            comm.append(name_bytes);
            comm.append(&[version_bytes.len() as u8]);
            comm.append(version_bytes);
            comm.append(&[0]); // No flags
            ui::SingleMessage::new("GetVersion... Done").show();
            comm.reply_ok();
        },
        Instruction::GetPrivateKey => {
            // first 5 bytes are instruction details
            let offset = 5;
            let mut address_index_bytes = [0u8; 8];
            address_index_bytes.clone_from_slice(comm.get(offset, offset + 8));
            let address_index = crate::u64_to_string(u64::from_le_bytes(address_index_bytes));

            let mut msg = "GetPrivateKey... ".to_string();
            msg.push_str(&address_index);
            ui::SingleMessage::new(&msg).show();

            let mut bip32_path = "m/44'/".to_string();
            bip32_path.push_str(&MINOTARI_LEDGER_ID.to_string());
            bip32_path.push_str(&"'/");
            bip32_path.push_str(&MINOTARI_ACCOUNT_ID.to_string());
            bip32_path.push_str(&"'/0/");
            bip32_path.push_str(&address_index);
            let path: [u32; 5] = nanos_sdk::ecc::make_bip32_path(bip32_path.as_bytes());

            let raw_key = get_raw_key(&path)?;

            let k = match RistrettoSecretKey::from_bytes(&raw_key) {
                Ok(val) => val,
                Err(_) => {
                    ui::SingleMessage::new("Err: key conversion").show();
                    return Err(SyscallError::InvalidParameter.into());
                },
            };
            comm.append(&[1]); // version
            comm.append(k.as_bytes());
            comm.reply_ok();
        },
        Instruction::BadInstruction(val) => {
            let mut error = "BadInstruction... ! (".to_string();
            error.push_str(&crate::byte_to_hex(val));
            error.push_str(&")");
            ui::SingleMessage::new(&error).show();
            return Err(StatusWords::BadIns.into());
        },
        Instruction::Exit => {
            ui::SingleMessage::new("Exit...").show();
            comm.reply_ok();
            nanos_sdk::exit_app(0)
        },
    }
    Ok(())
}
