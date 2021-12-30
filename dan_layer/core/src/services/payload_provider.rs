// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use async_trait::async_trait;

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{InstructionSet, Payload, TariDanPayload, TreeNodeHash},
    services::MempoolService,
};

#[async_trait]
pub trait PayloadProvider<TPayload: Payload> {
    async fn create_payload(&self) -> Result<TPayload, DigitalAssetError>;
    fn create_genesis_payload(&self) -> TPayload;
    async fn get_payload_queue(&self) -> usize;
    async fn reserve_payload(
        &mut self,
        payload: &TPayload,
        reservation_key: &TreeNodeHash,
    ) -> Result<(), DigitalAssetError>;
    async fn remove_payload(&mut self, reservation_key: &TreeNodeHash) -> Result<(), DigitalAssetError>;
}

pub struct TariDanPayloadProvider<TMempoolService: MempoolService> {
    mempool: TMempoolService,
}

impl<TMempoolService: MempoolService> TariDanPayloadProvider<TMempoolService> {
    pub fn new(mempool: TMempoolService) -> Self {
        Self { mempool }
    }
}

#[async_trait]
impl<TMempoolService: MempoolService> PayloadProvider<TariDanPayload> for TariDanPayloadProvider<TMempoolService> {
    async fn create_payload(&self) -> Result<TariDanPayload, DigitalAssetError> {
        let instructions = self.mempool.read_block(100).await?;
        let instruction_set = InstructionSet::from_slice(&instructions);

        Ok(TariDanPayload::new(instruction_set, None))
    }

    fn create_genesis_payload(&self) -> TariDanPayload {
        TariDanPayload::new(InstructionSet::empty(), None)
    }

    async fn get_payload_queue(&self) -> usize {
        self.mempool.size().await
    }

    async fn reserve_payload(
        &mut self,
        payload: &TariDanPayload,
        reservation_key: &TreeNodeHash,
    ) -> Result<(), DigitalAssetError> {
        // Reserve all instructions if they succeeded
        for instruction in payload.instructions() {
            self.mempool
                .reserve_instruction_in_block(instruction.hash(), reservation_key.0.clone())
                .await?;
        }
        Ok(())
    }

    async fn remove_payload(&mut self, reservation_key: &TreeNodeHash) -> Result<(), DigitalAssetError> {
        self.mempool.remove_all_in_block(reservation_key.as_bytes()).await
    }
}
