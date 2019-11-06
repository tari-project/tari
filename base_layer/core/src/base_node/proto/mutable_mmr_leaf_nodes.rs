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

use crate::base_node::proto::base_node::MutableMmrLeafNodes as ProtoMutableMmrLeafNodes;
use croaring::Bitmap;
use std::convert::TryFrom;
use tari_mmr::MutableMmrLeafNodes;

impl TryFrom<ProtoMutableMmrLeafNodes> for MutableMmrLeafNodes {
    type Error = String;

    fn try_from(state: ProtoMutableMmrLeafNodes) -> Result<Self, Self::Error> {
        let mut deleted = Bitmap::create();
        deleted.add_many(&state.deleted);
        Ok(Self {
            leaf_hashes: state.leaf_hashes.into_iter().map(Into::into).collect(),
            deleted,
        })
    }
}

impl From<MutableMmrLeafNodes> for ProtoMutableMmrLeafNodes {
    fn from(state: MutableMmrLeafNodes) -> Self {
        Self {
            leaf_hashes: state.leaf_hashes.into_iter().map(Into::into).collect(),
            deleted: state.deleted.to_vec(),
        }
    }
}
