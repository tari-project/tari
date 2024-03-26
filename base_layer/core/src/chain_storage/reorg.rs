//  Copyright 2022, The Tari Project
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

use std::{collections::VecDeque, sync::Arc};

use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_common_types::types::HashOutput;

use crate::blocks::ChainBlock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reorg {
    pub new_height: u64,
    pub new_hash: HashOutput,
    pub prev_height: u64,
    pub prev_hash: HashOutput,
    pub num_blocks_added: u64,
    pub num_blocks_removed: u64,
    pub local_time: NaiveDateTime,
}

impl Reorg {
    pub fn from_reorged_blocks(added: &VecDeque<Arc<ChainBlock>>, removed: &[Arc<ChainBlock>]) -> Self {
        // Expects blocks to be ordered sequentially highest height to lowest (as in rewind_to_height)
        Self {
            new_height: added.front().map(|b| b.header().height).unwrap_or_default(),
            new_hash: added.front().map(|b| *b.hash()).unwrap_or_default(),
            prev_height: removed.first().map(|b| b.header().height).unwrap_or_default(),
            prev_hash: removed.first().map(|b| *b.hash()).unwrap_or_default(),
            num_blocks_added: added.len() as u64,
            num_blocks_removed: removed.len() as u64,
            local_time: Utc::now().naive_local(),
        }
    }
}
