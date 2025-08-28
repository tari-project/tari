// Copyright 2019, The Tari Project
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

use std::convert::{TryFrom, TryInto};

use tari_common_types::types::PrivateKey;
use tari_node_components::blocks::{Block, NewBlock};
use tari_utilities::ByteArray;

use super::core as proto;
use crate::blocks::HistoricalBlock;
//---------------------------------- Block --------------------------------------------//

impl TryFrom<proto::Block> for Block {
    type Error = String;

    fn try_from(block: proto::Block) -> Result<Self, Self::Error> {
        let header = block
            .header
            .map(TryInto::try_into)
            .ok_or_else(|| "Block header not provided".to_string())??;

        let body = block
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Block body not provided".to_string())??;

        Ok(Self { header, body })
    }
}

impl TryFrom<Block> for proto::Block {
    type Error = String;

    fn try_from(block: Block) -> Result<Self, Self::Error> {
        Ok(Self {
            header: Some(block.header.into()),
            body: Some(block.body.try_into()?),
        })
    }
}

impl TryFrom<HistoricalBlock> for proto::HistoricalBlock {
    type Error = String;

    fn try_from(block: HistoricalBlock) -> Result<Self, Self::Error> {
        let (block, _accumulated_data, confirmations) = block.dissolve();
        Ok(Self {
            confirmations,
            // accumulated_data: Some(accumulated_data.into()),
            block: Some(block.try_into()?),
        })
    }
}

//---------------------------------- NewBlock --------------------------------------------//

impl TryFrom<proto::NewBlock> for NewBlock {
    type Error = String;

    fn try_from(new_block: proto::NewBlock) -> Result<Self, Self::Error> {
        let mut coinbase_kernels = Vec::new();
        for coinbase_kernel in new_block.coinbase_kernels {
            coinbase_kernels.push(coinbase_kernel.try_into()?)
        }
        let mut coinbase_outputs = Vec::new();
        for coinbase_output in new_block.coinbase_outputs {
            coinbase_outputs.push(coinbase_output.try_into()?)
        }
        Ok(Self {
            header: new_block.header.ok_or("No new block header provided")?.try_into()?,
            coinbase_kernels,
            coinbase_outputs,
            kernel_excess_sigs: new_block
                .kernel_excess_sigs
                .iter()
                .map(|bytes| PrivateKey::from_canonical_bytes(bytes))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Invalid excess signature scalar")?,
        })
    }
}

impl TryFrom<NewBlock> for proto::NewBlock {
    type Error = String;

    fn try_from(new_block: NewBlock) -> Result<Self, Self::Error> {
        let mut coinbase_kernels = Vec::new();
        for coinbase_kernel in new_block.coinbase_kernels {
            coinbase_kernels.push(coinbase_kernel.into())
        }
        let mut coinbase_outputs = Vec::new();
        for coinbase_output in new_block.coinbase_outputs {
            coinbase_outputs.push(coinbase_output.try_into()?)
        }
        Ok(Self {
            header: Some(new_block.header.into()),
            coinbase_kernels,
            coinbase_outputs,
            kernel_excess_sigs: new_block.kernel_excess_sigs.into_iter().map(|s| s.to_vec()).collect(),
        })
    }
}
