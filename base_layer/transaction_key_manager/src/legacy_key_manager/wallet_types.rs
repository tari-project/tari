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
use tari_common_types::{
    seeds::cipher_seed::CipherSeed,
    types::{CompressedPublicKey, PrivateKey},
};
use tari_transaction_components::key_manager::wallet_types::{
    LedgerWallet,
    SeedWordsWallet,
    SpendWallet,
    ViewWallet,
    WalletType,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default, Eq, PartialEq)]
pub enum LegacyWalletType {
    #[default]
    DerivedKeys,
    Ledger(LedgerWallet),
    ProvidedKeys(ProvidedKeysWallet),
}

impl LegacyWalletType {
    pub fn is_derived_keys(&self) -> bool {
        matches!(self, LegacyWalletType::DerivedKeys)
    }

    pub fn is_ledger(&self) -> bool {
        matches!(self, LegacyWalletType::Ledger(_))
    }

    pub fn is_provided_keys(&self) -> bool {
        matches!(self, LegacyWalletType::ProvidedKeys(_))
    }

    pub fn to_new_wallet_type(&self, master_seed: CipherSeed) -> Result<WalletType, String> {
        match self {
            LegacyWalletType::DerivedKeys => Ok(WalletType::SeedWords(
                SeedWordsWallet::construct_new(master_seed).map_err(|e| format!("{}", e))?,
            )),
            LegacyWalletType::Ledger(ledger_wallet) => Ok(WalletType::Ledger(ledger_wallet.clone())),
            LegacyWalletType::ProvidedKeys(provided_keys) => match &provided_keys.private_spend_key {
                Some(key) => Ok(WalletType::SpendWallet(SpendWallet::new(
                    key.clone(),
                    provided_keys.view_key.clone(),
                    provided_keys.birthday,
                ))),
                None => Ok(WalletType::ViewWallet(ViewWallet::new(
                    provided_keys.public_spend_key.clone(),
                    provided_keys.view_key.clone(),
                    provided_keys.birthday,
                ))),
            },
        }
    }
}

impl Display for LegacyWalletType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LegacyWalletType::DerivedKeys => write!(f, "Derived wallet"),
            LegacyWalletType::Ledger(ledger_wallet) => write!(f, "Ledger({ledger_wallet})"),
            LegacyWalletType::ProvidedKeys(provided_keys_wallet) => write!(f, "Provided Keys ({provided_keys_wallet})"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ProvidedKeysWallet {
    pub public_spend_key: CompressedPublicKey,
    pub private_spend_key: Option<PrivateKey>,
    pub private_comms_key: Option<PrivateKey>,
    pub view_key: PrivateKey,
    pub birthday: Option<u16>,
}

impl Display for ProvidedKeysWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "public spend key {}", self.public_spend_key)?;
        Ok(())
    }
}
