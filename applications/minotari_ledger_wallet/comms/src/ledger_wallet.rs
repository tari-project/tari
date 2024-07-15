// Copyright 2024 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::ops::Deref;

use ledger_transport::{APDUAnswer, APDUCommand};
use ledger_transport_hid::{hidapi::HidApi, TransportNativeHID};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use once_cell::sync::Lazy;
use tari_utilities::ByteArray;

use crate::error::LedgerDeviceError;

#[repr(u8)]
#[derive(FromPrimitive, Debug, Copy, Clone, PartialEq)]
pub enum Instruction {
    GetVersion = 0x01,
    GetAppName = 0x02,
    GetPublicAlpha = 0x03,
    GetPublicKey = 0x04,
    GetScriptSignature = 0x05,
    GetScriptOffset = 0x06,
    GetViewKey = 0x07,
    GetDHSharedSecret = 0x08,
    GetRawSchnorrSignature = 0x09,
    GetScriptSchnorrSignature = 0x10,
}

#[repr(u16)]
#[derive(FromPrimitive, Debug, Copy, Clone, PartialEq)]
pub enum AppSW {
    Deny = 0xB001,
    WrongP1P2 = 0xB002,
    InsNotSupported = 0xB003,
    ClaNotSupported = 0xB004,
    ScriptSignatureFail = 0xB005,
    MetadataSignatureFail = 0xB006,
    RawSchnorrSignatureFail = 0xB007,
    SchnorrSignatureFail = 0xB008,
    ScriptOffsetNotUnique = 0xB009,
    KeyDeriveFail = 0xB00A,
    KeyDeriveFromCanonical = 0xB00B,
    KeyDeriveFromUniform = 0xB00C,
    VersionParsingFail = 0xB00D,
    TooManyPayloads = 0xB00E,
    RandomNonceFail = 0xB00F,
    BadBranchKey = 0xB010,
    WrongApduLength = 0x6e03, // See ledger-device-rust-sdk/ledger_device_sdk/src/io.rs:16
    UserCancelled = 0x6e04,   // See ledger-device-rust-sdk/ledger_device_sdk/src/io.rs:16
}

impl From<u16> for AppSW {
    fn from(value: u16) -> Self {
        match value {
            0xB001 => AppSW::Deny,
            0xB002 => AppSW::WrongP1P2,
            0xB003 => AppSW::InsNotSupported,
            0xB004 => AppSW::ClaNotSupported,
            0xB005 => AppSW::ScriptSignatureFail,
            0xB006 => AppSW::MetadataSignatureFail,
            0xB007 => AppSW::RawSchnorrSignatureFail,
            0xB008 => AppSW::SchnorrSignatureFail,
            0xB009 => AppSW::ScriptOffsetNotUnique,
            0xB00A => AppSW::KeyDeriveFail,
            0xB00B => AppSW::KeyDeriveFromCanonical,
            0xB00C => AppSW::KeyDeriveFromUniform,
            0xB00D => AppSW::VersionParsingFail,
            0xB00E => AppSW::TooManyPayloads,
            0xB00F => AppSW::RandomNonceFail,
            0xB010 => AppSW::BadBranchKey,
            0x6e03 => AppSW::WrongApduLength,
            0x6e04 => AppSW::UserCancelled,
            _ => AppSW::Deny,
        }
    }
}

pub const EXPECTED_NAME: &str = "minotari_ledger_wallet";
pub const EXPECTED_VERSION: &str = "1.0.0-pre.16";
const WALLET_CLA: u8 = 0x80;

impl Instruction {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        FromPrimitive::from_u8(value)
    }
}

pub fn get_transport() -> Result<TransportNativeHID, LedgerDeviceError> {
    let hid = hidapi()?;
    let transport = TransportNativeHID::new(hid).map_err(|e| LedgerDeviceError::NativeTransport(e.to_string()))?;
    Ok(transport)
}

fn hidapi() -> Result<&'static HidApi, LedgerDeviceError> {
    static HIDAPI: Lazy<Result<HidApi, String>> =
        Lazy::new(|| HidApi::new().map_err(|e| format!("Unable to get HIDAPI: {}", e)));

    HIDAPI.as_ref().map_err(|e| LedgerDeviceError::HidApi(e.to_string()))
}

#[derive(Debug, Clone)]
pub struct Command<D> {
    inner: APDUCommand<D>,
}

impl<D: Deref<Target = [u8]>> Command<D> {
    pub fn new(inner: APDUCommand<D>) -> Command<D> {
        Self { inner }
    }

    pub fn execute(&self) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        get_transport()?
            .exchange(&self.inner)
            .map_err(|e| LedgerDeviceError::NativeTransport(e.to_string()))
    }

    pub fn execute_with_transport(
        &self,
        transport: &TransportNativeHID,
    ) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        transport
            .exchange(&self.inner)
            .map_err(|e| LedgerDeviceError::NativeTransport(e.to_string()))
    }

    pub fn build_command(account: u64, instruction: Instruction, data: Vec<u8>) -> Command<Vec<u8>> {
        let mut base_data = account.to_le_bytes().to_vec();
        base_data.extend_from_slice(&data);

        Command::new(APDUCommand {
            cla: WALLET_CLA,
            ins: instruction.as_byte(),
            p1: 0x00,
            p2: 0x00,
            data: base_data,
        })
    }

    pub fn chunk_command(account: u64, instruction: Instruction, data: Vec<Vec<u8>>) -> Vec<Command<Vec<u8>>> {
        let num_chunks = data.len();
        let mut more;
        let mut commands = vec![];

        for (i, chunk) in data.iter().enumerate() {
            if i + 1 == num_chunks {
                more = 0;
            } else {
                more = 1;
            }

            // Prepend the account on the first payload
            let mut base_data = vec![];
            if i == 0 {
                base_data.extend_from_slice(&account.to_le_bytes().to_vec());
            }
            base_data.extend_from_slice(chunk);

            commands.push(Command::new(APDUCommand {
                cla: WALLET_CLA,
                ins: instruction.as_byte(),
                p1: u8::try_from(i).unwrap_or(0),
                p2: more,
                data: base_data,
            }));
        }

        commands
    }
}
