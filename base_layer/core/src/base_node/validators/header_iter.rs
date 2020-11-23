//  Copyright 2020, The Tari Project
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
    blocks::BlockHeader,
    chain_storage::{BlockchainBackend, BlockchainDatabase, ChainStorageError},
};use crate::blocks::chain_header::ChainHeader;
use std::cmp;

// TODO: This is probably generally useful and should be included in the BlockchainDatabase
/// Iterator that emits BlockHeaders until a given height. This iterator loads headers in chunks of size `chunk_size`
/// for a low memory footprint. The chunk buffer is allocated once and reused.
pub struct HeaderIter<'a, B> {
    chunk: Vec<ChainHeader>,
    chunk_size: usize,
    cursor: usize,
    is_error: bool,
    height: u64,
    db: &'a BlockchainDatabase<B>,
}

impl<'a, B> HeaderIter<'a, B> {
    #[allow(dead_code)]
    pub fn new(db: &'a BlockchainDatabase<B>, height: u64, chunk_size: usize) -> Self {
        Self {
            db,
            chunk_size,
            cursor: 0,
            is_error: false,
            height,
            chunk: Vec::with_capacity(chunk_size),
        }
    }

    fn next_chunk(&self) -> (u64, u64) {
        let upper_bound = cmp::min(self.cursor + self.chunk_size, self.height as usize);
        (self.cursor as u64, upper_bound as u64)
    }
}

impl<B: BlockchainBackend> Iterator for HeaderIter<'_, B> {
    type Item = Result<ChainHeader, ChainStorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_error {
            return None;
        }

        if self.chunk.is_empty() {
            let (start, end) = self.next_chunk();
            // We're done: No more block headers to fetch
            if start > end {
                return None;
            }

            match self.db.fetch_headers(start..=end) {
                Ok(headers) => {
                    if headers.is_empty() {
                        return None;
                    }
                    self.cursor += headers.len();
                    self.chunk.extend(headers);
                },
                Err(err) => {
                    // On the next call, the iterator will end
                    self.is_error = true;
                    return Some(Err(err));
                },
            }
        }

        Some(Ok(self.chunk.remove(0)))
    }
}
