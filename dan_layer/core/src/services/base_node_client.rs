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
use tari_common_types::types::PublicKey;

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{AssetDefinition, BaseLayerMetadata, BaseLayerOutput},
};

#[async_trait]
pub trait BaseNodeClient {
    async fn get_tip_info(&mut self) -> Result<BaseLayerMetadata, DigitalAssetError>;

    async fn get_current_checkpoint(
        &mut self,
        height: u64,
        asset_public_key: PublicKey,
        checkpoint_unique_id: Vec<u8>,
    ) -> Result<Option<BaseLayerOutput>, DigitalAssetError>;

    async fn get_assets_for_dan_node(
        &mut self,
        dan_node_public_key: PublicKey,
    ) -> Result<Vec<AssetDefinition>, DigitalAssetError>;

    async fn get_asset_registration(
        &mut self,
        asset_public_key: PublicKey,
    ) -> Result<Option<BaseLayerOutput>, DigitalAssetError>;
}
