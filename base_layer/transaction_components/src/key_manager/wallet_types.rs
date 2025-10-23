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

use blake2::Blake2b;
use digest::consts::U64;
use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_common_types::{
    seeds::cipher_seed::CipherSeed,
    types::{CompressedPublicKey, PrivateKey, UncompressedPublicKey},
};
use tari_crypto::{
    hashing::DomainSeparatedHasher,
    keys::{PublicKey, SecretKey},
};
use tari_hashing::KeyManagerDomain;
use tari_utilities::ByteArrayError;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum WalletType {
    SeedWords(SeedWordsWallet),
    Ledger(LedgerWallet),
    ViewWallet(ViewWallet),
    SpendWallet(SpendWallet),
}

impl WalletType {
    pub fn is_ledger(&self) -> bool {
        matches!(self, WalletType::Ledger(_))
    }

    pub fn get_ledger_details(&self) -> Option<&LedgerWallet> {
        match self {
            WalletType::Ledger(ledger_wallet) => Some(ledger_wallet),
            _ => None,
        }
    }

    pub fn get_public_spend_key(&self) -> CompressedPublicKey {
        match self {
            WalletType::SeedWords(seed_words_wallet) => {
                CompressedPublicKey::from_secret_key(seed_words_wallet.spend_key())
            },
            WalletType::Ledger(ledger_wallet) => ledger_wallet.public_alpha.clone(),
            WalletType::ViewWallet(view_wallet) => view_wallet.public_spend_key().clone(),
            WalletType::SpendWallet(spend_wallet) => {
                CompressedPublicKey::from_secret_key(spend_wallet.private_spend_key())
            },
        }
    }

    pub fn get_private_spend_key(&self) -> Option<PrivateKey> {
        match self {
            WalletType::SeedWords(seed_words_wallet) => Some(seed_words_wallet.spend_key().clone()),
            WalletType::Ledger(_) => None,
            WalletType::ViewWallet(_) => None,
            WalletType::SpendWallet(spend_wallet) => Some(spend_wallet.private_spend_key().clone()),
        }
    }

    pub fn get_view_key(&self) -> &PrivateKey {
        match self {
            WalletType::SeedWords(seed_words_wallet) => seed_words_wallet.view_key(),
            WalletType::Ledger(ledger_wallet) => &ledger_wallet.view_key,
            WalletType::ViewWallet(view_wallet) => view_wallet.view_key(),
            WalletType::SpendWallet(spend_wallet) => spend_wallet.view_key(),
        }
    }

    pub fn get_public_view_key(&self) -> CompressedPublicKey {
        let view_key = self.get_view_key();
        CompressedPublicKey::from_secret_key(view_key)
    }
}

impl Display for WalletType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WalletType::SeedWords(seed_words_wallet) => write!(f, "Seed word wallet ({seed_words_wallet})"),
            WalletType::Ledger(ledger_wallet) => write!(f, "Ledger({ledger_wallet})"),
            WalletType::ViewWallet(view_wallet) => write!(f, "View only wallet ({view_wallet})"),
            WalletType::SpendWallet(spend_wallet) => write!(f, "Spend wallet ({spend_wallet})"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ViewWallet {
    public_spend_key: CompressedPublicKey,
    private_spend_key: Option<PrivateKey>,
    view_key: PrivateKey,
    birthday: Option<u16>,
}

impl ViewWallet {
    pub fn new_with_spend_key(private_spend_key: PrivateKey, view_key: PrivateKey, birthday: Option<u16>) -> Self {
        let public_spend_key = CompressedPublicKey::from_secret_key(&private_spend_key);
        Self {
            public_spend_key,
            private_spend_key: Some(private_spend_key),
            view_key,
            birthday,
        }
    }

    pub fn new(public_spend_key: CompressedPublicKey, view_key: PrivateKey, birthday: Option<u16>) -> Self {
        Self {
            public_spend_key,
            private_spend_key: None,
            view_key,
            birthday,
        }
    }

    pub fn public_spend_key(&self) -> &CompressedPublicKey {
        &self.public_spend_key
    }

    pub fn private_spend_key(&self) -> Option<&PrivateKey> {
        self.private_spend_key.as_ref()
    }

    pub fn view_key(&self) -> &PrivateKey {
        &self.view_key
    }

    pub fn birthday(&self) -> Option<u16> {
        self.birthday
    }
}

impl Display for ViewWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "public spend key {}", self.public_spend_key)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SpendWallet {
    private_spend_key: PrivateKey,
    view_key: PrivateKey,
    birthday: Option<u16>,
}

impl SpendWallet {
    pub fn new(private_spend_key: PrivateKey, view_key: PrivateKey, birthday: Option<u16>) -> Self {
        Self {
            private_spend_key,
            view_key,
            birthday,
        }
    }

    pub fn construct_new(
        cipher_seed: CipherSeed,
        spend_account: u64,
        view_account: u64,
        birthday: Option<u16>,
    ) -> Result<Self, ByteArrayError> {
        let view_key = derive_private_key(&cipher_seed, VIEW_KEY_BRANCH.to_string(), view_account)?;
        let private_spend_key = derive_private_key(&cipher_seed, SPEND_KEY_BRANCH.to_string(), spend_account)?;
        Ok(Self {
            private_spend_key,
            view_key,
            birthday,
        })
    }

    pub fn private_spend_key(&self) -> &PrivateKey {
        &self.private_spend_key
    }

    pub fn view_key(&self) -> &PrivateKey {
        &self.view_key
    }

    pub fn birthday(&self) -> Option<u16> {
        self.birthday
    }
}

impl Display for SpendWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let public_spend_key = UncompressedPublicKey::from_secret_key(&self.private_spend_key);
        write!(f, "public spend key {}", public_spend_key)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LedgerWallet {
    pub account: u64,
    pub public_alpha: CompressedPublicKey,
    pub network: Network,
    pub view_key: PrivateKey,
}

impl Display for LedgerWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "account '{}', ", self.account)?;
        write!(f, "network '{}', ", self.network)?;
        write!(f, "public_alpha '{}', ", self.public_alpha)?;
        Ok(())
    }
}

impl LedgerWallet {
    pub fn new(account: u64, network: Network, public_alpha: CompressedPublicKey, view_key: PrivateKey) -> Self {
        Self {
            account,
            public_alpha,
            network,
            view_key,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SeedWordsWallet {
    cipher_seed: CipherSeed,
    spend_key: PrivateKey,
    view_key: PrivateKey,
}
pub const HASHER_LABEL_DERIVE_KEY: &str = "derive_key";
pub type KeyDigest = Blake2b<U64>;
pub const VIEW_KEY_BRANCH: &str = "data encryption";
pub const SPEND_KEY_BRANCH: &str = "comms";

impl SeedWordsWallet {
    pub fn construct_new(cipher_seed: CipherSeed) -> Result<Self, ByteArrayError> {
        let view_key = derive_private_key(&cipher_seed, VIEW_KEY_BRANCH.to_string(), 0)?;
        let spend_key = derive_private_key(&cipher_seed, SPEND_KEY_BRANCH.to_string(), 0)?;
        Ok(Self {
            cipher_seed,
            spend_key,
            view_key,
        })
    }

    pub fn cipher_seed(&self) -> &CipherSeed {
        &self.cipher_seed
    }

    pub fn spend_key(&self) -> &PrivateKey {
        &self.spend_key
    }

    pub fn view_key(&self) -> &PrivateKey {
        &self.view_key
    }
}

impl Display for SeedWordsWallet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let public_spend_key = UncompressedPublicKey::from_secret_key(&self.spend_key);
        let public_view_key = UncompressedPublicKey::from_secret_key(&self.view_key);
        write!(f, "Spend key '{}', ", public_spend_key)?;
        write!(f, "View key '{}', ", public_view_key)?;
        Ok(())
    }
}

fn derive_private_key(seed: &CipherSeed, branch_seed: String, account: u64) -> Result<PrivateKey, ByteArrayError> {
    // apply domain separation to generate derive key. Under the hood, the hashing api prepends the length of each
    // piece of data for concatenation, reducing the risk of collisions due to redundancy of variable length
    // input
    let derive_key = DomainSeparatedHasher::<KeyDigest, KeyManagerDomain>::new_with_label(HASHER_LABEL_DERIVE_KEY)
        .chain(seed.entropy())
        .chain(branch_seed.as_bytes())
        .chain(account.to_le_bytes())
        .finalize();

    let derive_key = derive_key.as_ref();
    let s = PrivateKey::from_uniform_bytes(derive_key)?;
    Ok(s)
}
