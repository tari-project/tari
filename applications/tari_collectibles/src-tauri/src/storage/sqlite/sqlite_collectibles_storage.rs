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
use crate::storage::{
  sqlite::{
    sqlite_addresses_table_gateway::SqliteAddressesTableGateway,
    sqlite_asset_wallets_table_gateway::SqliteAssetWalletsTableGateway,
    sqlite_issued_assets_table_gateway::SqliteIssuedAssetsTableGateway,
    sqlite_key_indices_table_gateway::SqliteKeyIndicesTableGateway,
    sqlite_tip002_addresses_table_gateway::SqliteTip002AddressesTableGateway,
    sqlite_transaction::SqliteTransaction, SqliteAssetsTableGateway,
    SqliteTip721TokensTableGateway, SqliteWalletsTableGateway,
  },
  CollectiblesStorage, StorageError,
};
use diesel::{Connection, SqliteConnection};

pub struct SqliteCollectiblesStorage {
  pub database_url: String,
}

impl CollectiblesStorage for SqliteCollectiblesStorage {
  type Addresses = SqliteAddressesTableGateway;
  type Assets = SqliteAssetsTableGateway;
  type AssetWallets = SqliteAssetWalletsTableGateway;
  type IssuedAssets = SqliteIssuedAssetsTableGateway;
  type Tip002Addresses = SqliteTip002AddressesTableGateway;
  type KeyIndices = SqliteKeyIndicesTableGateway;
  type Wallets = SqliteWalletsTableGateway;
  type Tip721Tokens = SqliteTip721TokensTableGateway;
  type Transaction = SqliteTransaction;

  fn create_transaction(&self) -> Result<Self::Transaction, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    conn.execute("PRAGMA foreign_keys = ON;")?;
    conn.execute("BEGIN EXCLUSIVE TRANSACTION;")?;

    Ok(SqliteTransaction::new(conn))
  }

  fn addresses(&self) -> Self::Addresses {
    SqliteAddressesTableGateway {}
  }

  fn assets(&self) -> Self::Assets {
    SqliteAssetsTableGateway {
      database_url: self.database_url.clone(),
    }
  }

  fn asset_wallets(&self) -> Self::AssetWallets {
    SqliteAssetWalletsTableGateway {}
  }

  fn issued_assets(&self) -> Self::IssuedAssets {
    SqliteIssuedAssetsTableGateway {}
  }

  fn tip002_addresses(&self) -> Self::Tip002Addresses {
    SqliteTip002AddressesTableGateway {}
  }

  fn tip721_tokens(&self) -> Self::Tip721Tokens {
    SqliteTip721TokensTableGateway {}
  }

  fn key_indices(&self) -> Self::KeyIndices {
    SqliteKeyIndicesTableGateway {
      database_url: self.database_url.clone(),
    }
  }

  fn wallets(&self) -> Self::Wallets {
    SqliteWalletsTableGateway {
      database_url: self.database_url.clone(),
    }
  }
}
