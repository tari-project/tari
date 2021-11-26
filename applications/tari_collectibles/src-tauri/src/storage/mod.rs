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

pub mod models;
pub mod sqlite;
mod storage_error;

use crate::storage::models::{
  address_row::AddressRow, asset_row::AssetRow, asset_wallet_row::AssetWalletRow,
  key_index_row::KeyIndexRow, tip002_address_row::Tip002AddressRow, wallet_row::WalletRow,
};
pub use storage_error::StorageError;
use tari_common_types::types::PublicKey;
use tari_key_manager::cipher_seed::CipherSeed;
use uuid::Uuid;

pub trait StorageTransaction {
  fn commit(self) -> Result<(), StorageError>;
}

pub trait CollectiblesStorage {
  type Addresses: AddressesTableGateway<Self::Transaction>;
  type Assets: AssetsTableGateway<Self::Transaction>;
  type AssetWallets: AssetWalletsTableGateway<Self::Transaction>;
  type IssuedAssets: IssuedAssetsTableGateway;
  type Tip002Addresses: Tip002AddressesTableGateway<Self::Transaction>;
  type KeyIndices: KeyIndicesTableGateway<Self::Transaction>;
  type Wallets: WalletsTableGateway<Self::Transaction>;
  type Transaction: StorageTransaction;

  fn create_transaction(&self) -> Result<Self::Transaction, StorageError>;
  fn addresses(&self) -> Self::Addresses;
  fn assets(&self) -> Self::Assets;
  fn asset_wallets(&self) -> Self::AssetWallets;
  fn issued_assets(&self) -> Self::IssuedAssets;
  fn tip002_addresses(&self) -> Self::Tip002Addresses;
  fn key_indices(&self) -> Self::KeyIndices;
  fn wallets(&self) -> Self::Wallets;
}

pub trait AssetsTableGateway<T: StorageTransaction> {
  fn list(&self, tx: &T) -> Result<Vec<AssetRow>, StorageError>;
  fn insert(&self, asset: &AssetRow, tx: &T) -> Result<(), StorageError>;
  fn find(&self, asset_id: Uuid, tx: &T) -> Result<AssetRow, StorageError>;
  fn find_by_public_key(&self, public_key: &PublicKey, tx: &T) -> Result<AssetRow, StorageError>;
}

pub trait WalletsTableGateway<T: StorageTransaction> {
  type Passphrase;

  fn list(&self, tx: Option<&T>) -> Result<Vec<WalletRow>, StorageError>;
  fn insert(
    &self,
    wallet: &WalletRow,
    pass: Option<Self::Passphrase>,
    tx: &T,
  ) -> Result<(), StorageError>;
  fn find(&self, id: Uuid, tx: Option<&T>) -> Result<WalletRow, StorageError>;
  fn get_cipher_seed(
    &self,
    id: Uuid,
    pass: Option<Self::Passphrase>,
    tx: Option<&T>,
  ) -> Result<CipherSeed, StorageError>;
}

pub trait KeyIndicesTableGateway<T: StorageTransaction> {
  fn list(&self, tx: &T) -> Result<Vec<KeyIndexRow>, StorageError>;
  fn insert(&self, key_index: &KeyIndexRow, tx: &T) -> Result<(), StorageError>;
  fn update_last_index(
    &self,
    old_row: &KeyIndexRow,
    new_last_index: u64,
    tx: &T,
  ) -> Result<(), StorageError>;
  fn find(&self, branch_seed: String, tx: &T) -> Result<Option<KeyIndexRow>, StorageError>;
}

pub trait AssetWalletsTableGateway<T: StorageTransaction> {
  fn insert(&self, row: &AssetWalletRow, tx: &T) -> Result<(), StorageError>;
  fn find_by_wallet_id(&self, wallet_id: Uuid, tx: &T)
    -> Result<Vec<AssetWalletRow>, StorageError>;
}

pub trait AddressesTableGateway<T: StorageTransaction> {
  fn insert(&self, row: &AddressRow, tx: &T) -> Result<(), StorageError>;
  fn find_by_asset_and_wallet(
    &self,
    asset_id: Uuid,
    wallet_id: Uuid,
    tx: &T,
  ) -> Result<Vec<AddressRow>, StorageError>;
}

pub trait IssuedAssetsTableGateway {}

pub trait Tip002AddressesTableGateway<T: StorageTransaction> {
  fn insert(&self, row: &Tip002AddressRow, tx: &T) -> Result<(), StorageError>;
}
