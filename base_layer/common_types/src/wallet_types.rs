//  Copyright 2023 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
#[cfg(feature = "ledger")]
use std::convert::TryFrom;
use std::{
    fmt,
    fmt::{Display, Formatter},
};

use chacha20poly1305::aead::OsRng;
#[cfg(feature = "ledger")]
use ledger_transport::APDUCommand;
#[cfg(feature = "ledger")]
use minotari_ledger_wallet_comms::ledger_wallet::{Command, Instruction};
use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_crypto::{
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    ristretto::RistrettoPublicKey,
};

use crate::types::{PrivateKey, PublicKey};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalletType {
    Software(PrivateKey, PublicKey),
    Ledger(LedgerWallet),
}

impl Display for WalletType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WalletType::Software(_k, pk) => write!(f, "Software({:?})", pk),
            WalletType::Ledger(account) => write!(f, "Ledger({account})"),
        }
    }
}

impl Default for WalletType {
    fn default() -> Self {
        let k: PrivateKey = SecretKey::random(&mut OsRng);
        WalletType::Software(k.clone(), PublicKey::from_secret_key(&k))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerWallet {
    account: u64,
    pub public_alpha: Option<RistrettoPublicKey>,
    pub network: Network,
}

impl Display for LedgerWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "account {}", self.account)?;
        write!(f, "pubkey {}", self.public_alpha.is_some())?;
        Ok(())
    }
}

#[cfg(feature = "ledger")]
const WALLET_CLA: u8 = 0x80;

impl LedgerWallet {
    pub fn new(account: u64, network: Network, public_alpha: Option<RistrettoPublicKey>) -> Self {
        Self {
            account,
            public_alpha,
            network,
        }
    }

    pub fn account_bytes(&self) -> Vec<u8> {
        self.account.to_le_bytes().to_vec()
    }

    #[cfg(feature = "ledger")]
    pub fn build_command(&self, instruction: Instruction, data: Vec<u8>) -> Command<Vec<u8>> {
        let mut base_data = self.account_bytes();
        base_data.extend_from_slice(&data);

        Command::new(APDUCommand {
            cla: WALLET_CLA,
            ins: instruction.as_byte(),
            p1: 0x00,
            p2: 0x00,
            data: base_data,
        })
    }

    #[cfg(feature = "ledger")]
    pub fn chunk_command(&self, instruction: Instruction, data: Vec<Vec<u8>>) -> Vec<Command<Vec<u8>>> {
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
                base_data.extend_from_slice(&self.account_bytes());
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
