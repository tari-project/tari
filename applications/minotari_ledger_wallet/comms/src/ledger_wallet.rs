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

use std::{
    fmt,
    fmt::{Display, Formatter},
    ops::Deref,
};

use ledger_transport::{APDUAnswer, APDUCommand};
use ledger_transport_hid::{hidapi::HidApi, TransportNativeHID};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoPublicKey;

use crate::error::LedgerDeviceError;

const WALLET_CLA: u8 = 0x80;
// const LOG_TARGET: &str = "wallet::ledger_wallet::comms";

#[repr(u8)]
#[derive(FromPrimitive, Debug, Copy, Clone, PartialEq)]
pub enum Instruction {
    GetVersion = 0x01,
    GetAppName = 0x02,
    GetPrivateKey = 0x03,
    GetPublicKey = 0x04,
    GetScriptSignature = 0x05,
}

impl Instruction {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        FromPrimitive::from_u8(value)
    }
}

pub fn get_transport() -> Result<TransportNativeHID, LedgerDeviceError> {
    let hid = HidApi::new().map_err(|e| LedgerDeviceError::HidApi(e.to_string()))?;
    TransportNativeHID::new(&hid).map_err(|e| LedgerDeviceError::NativeTransport(e.to_string()))
}

#[derive(Debug, Clone)]
pub struct Command<D> {
    inner: APDUCommand<D>,
}

impl<D: Deref<Target = [u8]>> Command<D> {
    pub fn execute(&self) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        get_transport()?
            .exchange(&self.inner)
            .map_err(|e| LedgerDeviceError::NativeTransport(e.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerWallet {
    account: u64,
    pub pubkey: Option<RistrettoPublicKey>,
}

impl Display for LedgerWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "account {}", self.account)?;
        write!(f, "pubkey {}", self.pubkey.is_some())?;
        Ok(())
    }
}

impl LedgerWallet {
    pub fn new(account: u64, pubkey: Option<RistrettoPublicKey>) -> Self {
        Self { account, pubkey }
    }

    pub fn account_bytes(&self) -> Vec<u8> {
        self.account.to_le_bytes().to_vec()
    }

    pub fn build_command(&self, instruction: Instruction, data: Vec<u8>) -> Command<Vec<u8>> {
        let mut base_data = self.account_bytes();
        base_data.extend_from_slice(&data);

        Command {
            inner: APDUCommand {
                cla: WALLET_CLA,
                ins: instruction.as_byte(),
                p1: 0x00,
                p2: 0x00,
                data: base_data,
            },
        }
    }
}
