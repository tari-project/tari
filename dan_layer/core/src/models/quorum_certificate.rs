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

use crate::{
    models::{HotStuffMessageType, Signature, TreeNodeHash, ViewId},
    storage::chain::DbQc,
};

#[derive(Debug, Clone)]
pub struct QuorumCertificate {
    message_type: HotStuffMessageType,
    node_hash: TreeNodeHash,
    view_number: ViewId,
    signature: Option<Signature>,
}

impl QuorumCertificate {
    pub fn new(
        message_type: HotStuffMessageType,
        view_number: ViewId,
        node_hash: TreeNodeHash,
        signature: Option<Signature>,
    ) -> Self {
        Self {
            message_type,
            node_hash,
            view_number,
            signature,
        }
    }

    pub fn genesis(node_hash: TreeNodeHash) -> Self {
        Self {
            message_type: HotStuffMessageType::Genesis,
            node_hash,
            view_number: 0.into(),
            signature: None,
        }
    }

    pub fn node_hash(&self) -> &TreeNodeHash {
        &self.node_hash
    }

    pub fn view_number(&self) -> ViewId {
        self.view_number
    }

    pub fn message_type(&self) -> HotStuffMessageType {
        self.message_type
    }

    pub fn signature(&self) -> Option<&Signature> {
        self.signature.as_ref()
    }

    pub fn combine_sig(&mut self, partial_sig: &Signature) {
        self.signature = match &self.signature {
            None => Some(partial_sig.clone()),
            Some(s) => Some(s.combine(partial_sig)),
        };
    }

    pub fn matches(&self, message_type: HotStuffMessageType, view_id: ViewId) -> bool {
        // from hotstuf spec
        self.message_type() == message_type && view_id == self.view_number()
    }
}

impl From<DbQc> for QuorumCertificate {
    fn from(rec: DbQc) -> Self {
        Self {
            message_type: rec.message_type,
            node_hash: rec.node_hash,
            view_number: rec.view_number,
            signature: rec.signature,
        }
    }
}
