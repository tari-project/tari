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

use std::{ops::Deref, sync::Mutex};

use ledger_transport::{APDUAnswer, APDUCommand};
use ledger_transport_hid::{hidapi::HidApi, TransportNativeHID};
use minotari_ledger_wallet_common::common_types::Instruction;
use once_cell::sync::Lazy;
use tari_utilities::ByteArray;

use crate::error::LedgerDeviceError;

pub const EXPECTED_NAME: &str = "minotari_ledger_wallet";
pub const EXPECTED_VERSION: &str = env!("CARGO_PKG_VERSION");
const WALLET_CLA: u8 = 0x80;

struct HidManager {
    inner: Option<HidApi>,
}

impl HidManager {
    fn new() -> Result<Self, LedgerDeviceError> {
        let hidapi = HidApi::new().map_err(|e| LedgerDeviceError::HidApi(e.to_string()))?;
        Ok(Self { inner: Some(hidapi) })
    }

    fn refresh_if_needed(&mut self) -> Result<(), LedgerDeviceError> {
        // We need to clear out the inner HidApi instance before creating a new one
        // This is because only one instance may exist, even when it no longers holds a connection,
        // and we want this dropped before replacing
        self.inner = None;

        self.inner = Some(HidApi::new().map_err(|e| LedgerDeviceError::HidApiRefresh(e.to_string()))?);

        Ok(())
    }

    fn get_hidapi(&self) -> Option<&HidApi> {
        self.inner.as_ref()
    }
}

static HID_MANAGER: Lazy<Mutex<HidManager>> =
    Lazy::new(|| Mutex::new(HidManager::new().expect("Failed to initialize HidManager")));

pub fn get_transport() -> Result<TransportNativeHID, LedgerDeviceError> {
    let mut manager = HID_MANAGER
        .lock()
        .map_err(|_| LedgerDeviceError::NativeTransport("Mutex lock error".to_string()))?;

    match TransportNativeHID::new(manager.get_hidapi().unwrap()) {
        Ok(transport) => Ok(transport),
        Err(_) => {
            manager.refresh_if_needed()?;
            TransportNativeHID::new(manager.get_hidapi().unwrap())
                .map_err(|e| LedgerDeviceError::NativeTransport(e.to_string()))
        },
    }
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
            .map_err(|e| LedgerDeviceError::NativeTransportExchange(e.to_string()))
    }

    pub fn execute_with_transport(
        &self,
        transport: &TransportNativeHID,
    ) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        transport
            .exchange(&self.inner)
            .map_err(|e| LedgerDeviceError::NativeTransportExchange(e.to_string()))
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
