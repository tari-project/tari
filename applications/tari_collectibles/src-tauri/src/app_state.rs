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

use crate::{
  clients::{BaseNodeClient, GrpcValidatorNodeClient, WalletClient},
  error::CollectiblesError,
  providers::ConcreteKeyManagerProvider,
  storage::{
    sqlite::{SqliteCollectiblesStorage, SqliteDbFactory},
    StorageError,
  },
};
use std::{path::PathBuf, sync::Arc};

use tari_common::configuration::CollectiblesConfig;
use tauri::async_runtime::RwLock;
use uuid::Uuid;

pub struct AppState {
  config: CollectiblesConfig,
  db_factory: SqliteDbFactory,
  current_wallet_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct ConcurrentAppState {
  inner: Arc<RwLock<AppState>>,
}

impl ConcurrentAppState {
  pub fn new(base_path: PathBuf, config: CollectiblesConfig) -> Self {
    let db_factory = SqliteDbFactory::new(base_path.as_path());

    Self {
      inner: Arc::new(RwLock::new(AppState {
        config,
        db_factory,
        current_wallet_id: None,
      })),
    }
  }

  pub async fn create_wallet_client(&self) -> WalletClient {
    WalletClient::new(
      self
        .inner
        .read()
        .await
        .config
        .wallet_grpc_address
        .clone()
        .to_string(),
    )
  }

  pub async fn connect_base_node_client(&self) -> Result<BaseNodeClient, CollectiblesError> {
    let lock = self.inner.read().await;
    let client =
      BaseNodeClient::connect(format!("http://{}", lock.config.base_node_grpc_address)).await?;
    Ok(client)
  }

  pub async fn connect_validator_node_client(
    &self,
  ) -> Result<GrpcValidatorNodeClient, CollectiblesError> {
    // todo: convert this GRPC to tari comms
    let lock = self.inner.read().await;
    let client = GrpcValidatorNodeClient::connect(format!(
      "http://{}",
      lock.config.validator_node_grpc_address
    ))
    .await?;
    Ok(client)
  }

  pub async fn create_db(&self) -> Result<SqliteCollectiblesStorage, StorageError> {
    let inner = self.inner.read().await;
    inner.db_factory.migrate()?;
    inner.db_factory.create_db()
  }

  pub async fn key_manager(&self) -> ConcreteKeyManagerProvider {
    let db_factory = self.inner.read().await.db_factory.clone();
    ConcreteKeyManagerProvider::new(db_factory)
  }

  pub async fn current_wallet_id(&self) -> Option<Uuid> {
    self.inner.read().await.current_wallet_id
  }

  pub async fn set_current_wallet_id(&self, wallet_id: Uuid) {
    self.inner.write().await.current_wallet_id = Some(wallet_id)
  }
}
