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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

// This file is used to store the genesis block
use crate::{
    blocks::{aggregated_body::AggregateBody, block::Block, blockheader::BlockHeader},
    types::*,
};

use chrono::{DateTime, NaiveDate, Utc};
use tari_crypto::ristretto::*;

pub fn get_genesis_block() -> Block {
    let blockheaders = get_gen_header();
    let body = get_gen_body();
    Block {
        header: blockheaders,
        body,
    }
}

pub fn get_gen_header() -> BlockHeader {
    BlockHeader {
        version: 0,
        /// Height of this block since the genesis block (height 0)
        height: 0,
        /// Hash of the block previous to this in the chain.
        prev_hash: [0; 32],
        /// Timestamp at which the block was built.
        timestamp: DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2020, 1, 1).and_hms(1, 1, 1), Utc),
        /// This is the MMR root of the outputs
        output_mr: [0; 32],
        /// This is the MMR root of the range proofs
        range_proof_mr: [0; 32],
        /// This is the MMR root of the kernels
        kernel_mr: [0; 32],
        /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
        /// block from the total kernel offset of the previous block header.
        total_kernel_offset: RistrettoSecretKey::from(0),
        /// Nonce used
        /// Proof of work summary
        pow: ProofOfWork::default(),
    }
}

pub fn get_gen_body() -> AggregateBody {
    AggregateBody::empty()
}
