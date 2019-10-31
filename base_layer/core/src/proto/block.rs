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

use super::types as proto;
use crate::{
    blocks::{aggregated_body::AggregateBody, Block, BlockHeader},
    chain_storage::HistoricalBlock,
    proto::utils::try_convert_all,
    types::BlindingFactor,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use prost_types::Timestamp;
use std::convert::{TryFrom, TryInto};
use tari_utilities::{ByteArray, ByteArrayError};

/// Utility function that converts a `prost::Timestamp` to a `chrono::DateTime`
pub(crate) fn timestamp_to_datetime(timestamp: Timestamp) -> DateTime<Utc> {
    let dt = NaiveDateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32);
    DateTime::<_>::from_utc(dt, Utc)
}

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
pub(crate) fn datetime_to_timestamp(datetime: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: datetime.timestamp(),
        nanos: datetime.timestamp_subsec_nanos() as i32,
    }
}

//---------------------------------- Block --------------------------------------------//

impl TryFrom<proto::Block> for Block {
    type Error = String;

    fn try_from(block: proto::Block) -> Result<Self, Self::Error> {
        let header = block
            .header
            .map(TryInto::try_into)
            .ok_or("Block header not provided".to_string())??;

        let body = block
            .body
            .map(TryInto::try_into)
            .ok_or("Block body not provided".to_string())??;

        Ok(Self { header, body })
    }
}

impl From<Block> for proto::Block {
    fn from(block: Block) -> Self {
        Self {
            header: Some(block.header.into()),
            body: Some(block.body.into()),
        }
    }
}

//---------------------------------- BlockHeader --------------------------------------------//

impl TryFrom<proto::BlockHeader> for BlockHeader {
    type Error = String;

    fn try_from(header: proto::BlockHeader) -> Result<Self, Self::Error> {
        let total_kernel_offset = BlindingFactor::from_bytes(
            &header
                .total_kernel_offset
                .ok_or("total_kernel_offset not provided".to_string())?
                .scalar,
        )
        .map_err(|err| err.to_string())?;

        let timestamp = header
            .timestamp
            .map(timestamp_to_datetime)
            .ok_or("timestamp not provided".to_string())?;

        Ok(Self {
            version: header.version as u16,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp,
            output_mr: header.output_mr,
            range_proof_mr: header.range_proof_mr,
            kernel_mr: header.kernel_mr,
            total_kernel_offset,
            total_difficulty: header.total_difficulty.into(),
            nonce: header.nonce,
            pow: Default::default(),
        })
    }
}

impl From<BlockHeader> for proto::BlockHeader {
    fn from(header: BlockHeader) -> Self {
        Self {
            version: header.version as u32,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp: Some(datetime_to_timestamp(header.timestamp)),
            output_mr: header.output_mr,
            range_proof_mr: header.range_proof_mr,
            kernel_mr: header.kernel_mr,
            total_kernel_offset: Some(header.total_kernel_offset.into()),
            total_difficulty: header.total_difficulty.as_u64(),
            nonce: header.nonce,
        }
    }
}

//---------------------------------- AggregateBody --------------------------------------------//

impl TryFrom<proto::AggregateBody> for AggregateBody {
    type Error = String;

    fn try_from(body: proto::AggregateBody) -> Result<Self, Self::Error> {
        Ok(Self {
            sorted: body.sorted,
            inputs: try_convert_all(body.inputs)?,
            outputs: try_convert_all(body.outputs)?,
            kernels: try_convert_all(body.kernels)?,
        })
    }
}

impl From<AggregateBody> for proto::AggregateBody {
    fn from(body: AggregateBody) -> Self {
        Self {
            sorted: body.sorted,
            inputs: body.inputs.into_iter().map(Into::into).collect(),
            outputs: body.outputs.into_iter().map(Into::into).collect(),
            kernels: body.kernels.into_iter().map(Into::into).collect(),
        }
    }
}

//---------------------------------- HistoricalBlock --------------------------------------------//

impl TryFrom<proto::HistoricalBlock> for HistoricalBlock {
    type Error = String;

    fn try_from(historical_block: proto::HistoricalBlock) -> Result<Self, Self::Error> {
        let spent_commitments =
            try_convert_all(historical_block.spent_commitments).map_err(|err: ByteArrayError| err.to_string())?;

        let block = historical_block
            .block
            .map(TryInto::try_into)
            .ok_or("block in historical block not provided".to_string())??;

        Ok(Self {
            confirmations: historical_block.confirmations,
            spent_commitments,
            block,
        })
    }
}

impl From<HistoricalBlock> for proto::HistoricalBlock {
    fn from(block: HistoricalBlock) -> Self {
        Self {
            confirmations: block.confirmations,
            spent_commitments: block.spent_commitments.into_iter().map(Into::into).collect(),
            block: Some(block.block.into()),
        }
    }
}
