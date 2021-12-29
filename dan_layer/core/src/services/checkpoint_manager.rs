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

use async_trait::async_trait;

use crate::{
    models::{AssetDefinition, StateRoot},
    services::wallet_client::WalletClient,
    DigitalAssetError,
};

#[async_trait]
pub trait CheckpointManager {
    async fn create_checkpoint(&mut self, state_root: StateRoot) -> Result<(), DigitalAssetError>;
}

#[derive(Default)]
pub struct ConcreteCheckpointManager<TWallet: WalletClient> {
    asset_definition: AssetDefinition,
    wallet: TWallet,
}

impl<TWallet: WalletClient> ConcreteCheckpointManager<TWallet> {
    pub fn new(asset_definition: AssetDefinition, wallet: TWallet) -> Self {
        Self {
            asset_definition,
            wallet,
        }
    }
}

#[async_trait]
impl<TWallet: WalletClient + Sync + Send> CheckpointManager for ConcreteCheckpointManager<TWallet> {
    async fn create_checkpoint(&mut self, state_root: StateRoot) -> Result<(), DigitalAssetError> {
        self.wallet
            .create_new_checkpoint(
                &self.asset_definition.public_key,
                &self.asset_definition.checkpoint_unique_id,
                &state_root,
            )
            .await
    }
}
