//  Copyright 2021. The Tari Project
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

use crate::models::{Account, NewAccount, NewWallet, Wallet, WalletInfo};
pub mod sqlite;
mod storage_error;
pub use storage_error::StorageError;
use uuid::Uuid;

pub trait CollectiblesStorage {
  type Accounts: AccountsTableGateway;
  type Wallets: WalletsTableGateway;

  fn accounts(&self) -> Self::Accounts;
  fn wallets(&self) -> Self::Wallets;
}

pub trait AccountsTableGateway {
  fn list(&self) -> Result<Vec<Account>, StorageError>;
  fn insert(&self, account: NewAccount) -> Result<Account, StorageError>;
  fn find(&self, account_id: Uuid) -> Result<Account, StorageError>;
}

pub trait WalletsTableGateway {
  type Passphrase;

  fn list(&self) -> Result<Vec<WalletInfo>, StorageError>;
  fn insert(&self, wallet: NewWallet, pass: Self::Passphrase) -> Result<Wallet, StorageError>;
  fn find(&self, id: Uuid, pass: Self::Passphrase) -> Result<Wallet, StorageError>;
}
