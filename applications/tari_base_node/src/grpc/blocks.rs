// Copyright 2019. The Tari Project
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

use tari_core::{base_node::LocalNodeCommsInterface, blocks::BlockHeader, chain_storage::HistoricalBlock};
use tonic::Status;

// The maximum number of blocks that can be requested at a time. These will be streamed to the
// client, so memory is not really a concern here, but a malicious client could request a large
// number here to keep the node busy
pub const GET_BLOCKS_MAX_HEIGHTS: usize = 1000;

// The number of blocks to request from the base node at a time. This is to reduce the number of
// requests to the base node, but if you'd like to stream directly, this can be set to 1.
pub const GET_BLOCKS_PAGE_SIZE: usize = 10;

/// Magic number for input and output sizes
pub const BLOCK_INPUT_SIZE: u64 = 4;
pub const BLOCK_OUTPUT_SIZE: u64 = 13;

/// Returns the block heights based on the start and end heights or from_tip
pub async fn block_heights(
    handler: LocalNodeCommsInterface,
    start_height: u64,
    end_height: u64,
    from_tip: u64,
) -> Result<Vec<u64>, Status>
{
    if end_height > 0 {
        Ok(BlockHeader::get_height_range(start_height, end_height))
    } else if from_tip > 0 {
        BlockHeader::get_heights_from_tip(handler, from_tip)
            .await
            .map_err(|e| Status::internal(e.to_string()))
    } else {
        Err(Status::invalid_argument("Invalid arguments provided"))
    }
}

pub fn block_size(block: &HistoricalBlock) -> u64 {
    let body = block.clone().block.body;

    let input_size = body.inputs().len() as u64 * BLOCK_INPUT_SIZE;
    let output_size = body.outputs().len() as u64 * BLOCK_OUTPUT_SIZE;
    input_size + output_size
}

pub fn block_fees(block: &HistoricalBlock) -> u64 {
    let body = block.clone().block.body;
    body.kernels()
        .iter()
        .map(|k| k.fee.into())
        .collect::<Vec<u64>>()
        .iter()
        .sum::<u64>()
}
