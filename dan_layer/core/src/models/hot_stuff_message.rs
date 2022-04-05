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

use digest::Digest;
use tari_common_types::types::PublicKey;
use tari_crypto::common::Blake256;

use crate::models::{
    HotStuffMessageType,
    HotStuffTreeNode,
    Payload,
    QuorumCertificate,
    Signature,
    TreeNodeHash,
    ViewId,
};

#[derive(Debug, Clone)]
pub struct HotStuffMessage<TPayload: Payload> {
    view_number: ViewId,
    message_type: HotStuffMessageType,
    justify: Option<QuorumCertificate>,
    node: Option<HotStuffTreeNode<TPayload>>,
    node_hash: Option<TreeNodeHash>,
    partial_sig: Option<Signature>,
    asset_public_key: PublicKey,
}

impl<TPayload: Payload> HotStuffMessage<TPayload> {
    pub fn new(
        view_number: ViewId,
        message_type: HotStuffMessageType,
        justify: Option<QuorumCertificate>,
        node: Option<HotStuffTreeNode<TPayload>>,
        node_hash: Option<TreeNodeHash>,
        partial_sig: Option<Signature>,
        asset_public_key: PublicKey,
    ) -> Self {
        HotStuffMessage {
            view_number,
            message_type,
            justify,
            node,
            node_hash,
            partial_sig,
            asset_public_key,
        }
    }

    pub fn new_view(prepare_qc: QuorumCertificate, view_number: ViewId, asset_public_key: PublicKey) -> Self {
        Self {
            message_type: HotStuffMessageType::NewView,
            view_number,
            justify: Some(prepare_qc),
            node: None,
            partial_sig: None,
            node_hash: None,
            asset_public_key,
        }
    }

    pub fn prepare(
        proposal: HotStuffTreeNode<TPayload>,
        high_qc: Option<QuorumCertificate>,
        view_number: ViewId,
        asset_public_key: PublicKey,
    ) -> Self {
        Self {
            message_type: HotStuffMessageType::Prepare,
            node: Some(proposal),
            justify: high_qc,
            view_number,
            partial_sig: None,
            node_hash: None,
            asset_public_key,
        }
    }

    pub fn vote_prepare(node_hash: TreeNodeHash, view_number: ViewId, asset_public_key: PublicKey) -> Self {
        Self {
            message_type: HotStuffMessageType::Prepare,
            node_hash: Some(node_hash),
            view_number,
            node: None,
            partial_sig: None,
            justify: None,
            asset_public_key,
        }
    }

    pub fn pre_commit(
        node: Option<HotStuffTreeNode<TPayload>>,
        prepare_qc: Option<QuorumCertificate>,
        view_number: ViewId,
        asset_public_key: PublicKey,
    ) -> Self {
        Self {
            message_type: HotStuffMessageType::PreCommit,
            node,
            justify: prepare_qc,
            view_number,
            node_hash: None,
            partial_sig: None,
            asset_public_key,
        }
    }

    pub fn vote_pre_commit(node_hash: TreeNodeHash, view_number: ViewId, asset_public_key: PublicKey) -> Self {
        Self {
            message_type: HotStuffMessageType::PreCommit,
            node_hash: Some(node_hash),
            view_number,
            node: None,
            partial_sig: None,
            justify: None,
            asset_public_key,
        }
    }

    pub fn commit(
        node: Option<HotStuffTreeNode<TPayload>>,
        pre_commit_qc: Option<QuorumCertificate>,
        view_number: ViewId,
        asset_public_key: PublicKey,
    ) -> Self {
        Self {
            message_type: HotStuffMessageType::Commit,
            node,
            justify: pre_commit_qc,
            view_number,
            partial_sig: None,
            node_hash: None,
            asset_public_key,
        }
    }

    pub fn vote_commit(node_hash: TreeNodeHash, view_number: ViewId, asset_public_key: PublicKey) -> Self {
        Self {
            message_type: HotStuffMessageType::Commit,
            node_hash: Some(node_hash),
            view_number,
            node: None,
            partial_sig: None,
            justify: None,
            asset_public_key,
        }
    }

    pub fn decide(
        node: Option<HotStuffTreeNode<TPayload>>,
        commit_qc: Option<QuorumCertificate>,
        view_number: ViewId,
        asset_public_key: PublicKey,
    ) -> Self {
        Self {
            message_type: HotStuffMessageType::Commit,
            node,
            justify: commit_qc,
            view_number,
            partial_sig: None,
            node_hash: None,
            asset_public_key,
        }
    }

    pub fn create_signature_challenge(&self) -> Vec<u8> {
        let mut b = Blake256::new()
            .chain(&[self.message_type.as_u8()])
            .chain(self.view_number.as_u64().to_le_bytes());
        if let Some(ref node) = self.node {
            b = b.chain(node.calculate_hash().as_bytes());
        } else if let Some(ref node_hash) = self.node_hash {
            b = b.chain(node_hash.as_bytes());
        } else {
        }
        b.finalize().to_vec()
    }

    pub fn view_number(&self) -> ViewId {
        self.view_number
    }

    pub fn asset_public_key(&self) -> &PublicKey {
        &self.asset_public_key
    }

    pub fn node(&self) -> Option<&HotStuffTreeNode<TPayload>> {
        self.node.as_ref()
    }

    pub fn node_hash(&self) -> Option<&TreeNodeHash> {
        self.node_hash.as_ref()
    }

    pub fn message_type(&self) -> HotStuffMessageType {
        self.message_type
    }

    pub fn justify(&self) -> Option<&QuorumCertificate> {
        self.justify.as_ref()
    }

    pub fn matches(&self, message_type: HotStuffMessageType, view_id: ViewId) -> bool {
        // from hotstuf spec
        self.message_type() == message_type && view_id == self.view_number()
    }

    pub fn add_partial_sig(&mut self, signature: Signature) {
        self.partial_sig = Some(signature)
    }

    pub fn partial_sig(&self) -> Option<&Signature> {
        self.partial_sig.as_ref()
    }
}
