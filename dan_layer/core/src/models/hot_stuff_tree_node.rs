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

use crate::models::{Payload, TreeNodeHash};
use digest::Digest;
use tari_crypto::common::Blake256;

#[derive(Debug, Clone)]
pub struct HotStuffTreeNode<TPayload: Payload> {
    parent: TreeNodeHash,
    payload: TPayload,
    hash: TreeNodeHash,
    height: u32,
}

impl<TPayload: Payload> HotStuffTreeNode<TPayload> {
    pub fn new(parent: TreeNodeHash, payload: TPayload, height: u32) -> Self {
        let mut s = HotStuffTreeNode {
            parent,
            payload,
            hash: TreeNodeHash(vec![]),
            height,
        };
        s.hash = s.calculate_hash();
        s
    }

    pub fn genesis(payload: TPayload) -> HotStuffTreeNode<TPayload> {
        let mut s = Self {
            parent: TreeNodeHash(vec![0u8; 32]),
            payload,
            hash: TreeNodeHash(vec![]),
            height: 0,
        };
        s.hash = s.calculate_hash();
        s
    }

    pub fn from_parent(parent: TreeNodeHash, payload: TPayload, height: u32) -> HotStuffTreeNode<TPayload> {
        Self::new(parent, payload, height)
    }

    pub fn calculate_hash(&self) -> TreeNodeHash {
        let result = Blake256::new()
            .chain(self.parent.0.as_slice())
            .chain(self.payload.consensus_hash())
            .chain(self.height.to_le_bytes())
            .finalize();
        TreeNodeHash(result.to_vec())
    }

    pub fn hash(&self) -> &TreeNodeHash {
        &self.hash
    }

    pub fn parent(&self) -> &TreeNodeHash {
        &self.parent
    }

    pub fn payload(&self) -> &TPayload {
        &self.payload
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl<TPayload: Payload> PartialEq for HotStuffTreeNode<TPayload> {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}
