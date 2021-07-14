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

use crate::dan_layer::models::{Payload, TreeNodeHash};
use digest::Digest;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use tari_crypto::common::Blake256;

#[derive(Debug, Clone, Hash)]
pub struct HotStuffTreeNode<TPayload: Payload> {
    parent: TreeNodeHash,
    payload: TPayload,
}

impl<TPayload: Payload> HotStuffTreeNode<TPayload> {
    pub fn genesis(payload: TPayload) -> HotStuffTreeNode<TPayload> {
        Self {
            parent: TreeNodeHash(vec![0u8; 32]),
            payload,
        }
    }

    pub fn from_parent(parent: &HotStuffTreeNode<TPayload>, payload: TPayload) -> HotStuffTreeNode<TPayload> {
        Self {
            parent: parent.calculate_hash(),
            payload,
        }
    }

    pub fn calculate_hash(&self) -> TreeNodeHash {
        let result = Blake256::new()
            .chain(self.parent.0.as_slice())
            .chain(self.payload.as_ref())
            .finalize();
        TreeNodeHash(result.to_vec())
    }

    pub fn parent(&self) -> &TreeNodeHash {
        &self.parent
    }
}
