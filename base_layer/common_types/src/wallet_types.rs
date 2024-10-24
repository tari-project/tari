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

use std::{
    fmt,
    fmt::{Display, Formatter},
};

use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_crypto::keys::PublicKey as PublicKeyTrait;

use crate::types::{PrivateKey, PublicKey};

#[derive(Debug, Clone, Serialize, Deserialize, Default, Eq, PartialEq)]
pub enum WalletType {
    #[default]
    DerivedKeys,
    Ledger(LedgerWallet),
    ProvidedKeys(ProvidedKeysWallet),
}

impl Display for WalletType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WalletType::DerivedKeys => write!(f, "Derived wallet"),
            WalletType::Ledger(ledger_wallet) => write!(f, "Ledger({ledger_wallet})"),
            WalletType::ProvidedKeys(provided_keys_wallet) => write!(f, "Provided Keys ({provided_keys_wallet})"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ProvidedKeysWallet {
    pub public_spend_key: PublicKey,
    pub private_spend_key: Option<PrivateKey>,
    pub private_comms_key: Option<PrivateKey>,
    pub view_key: PrivateKey,
}

impl Display for ProvidedKeysWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "public spend key {}", self.public_spend_key)?;
        write!(f, "public view key{}", PublicKey::from_secret_key(&self.view_key))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LedgerWallet {
    pub account: u64,
    pub public_alpha: Option<PublicKey>,
    pub network: Network,
    pub view_key: Option<PrivateKey>,
}

impl Display for LedgerWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "account '{}', ", self.account)?;
        write!(f, "network '{}', ", self.network)?;
        write!(f, "public_alpha '{}', ", self.public_alpha.is_some())?;
        write!(f, "view_key '{}'", self.view_key.is_some())?;
        Ok(())
    }
}

impl LedgerWallet {
    pub fn new(account: u64, network: Network, public_alpha: Option<PublicKey>, view_key: Option<PrivateKey>) -> Self {
        Self {
            account,
            public_alpha,
            network,
            view_key,
        }
    }
}
