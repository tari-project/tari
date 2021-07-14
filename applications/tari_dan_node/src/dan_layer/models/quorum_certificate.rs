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

use crate::dan_layer::models::{HotStuffMessageType, HotStuffTreeNode, Payload, Signature, ViewId};

#[derive(Debug, Clone)]
pub struct QuorumCertificate<TPayload: Payload> {
    message_type: HotStuffMessageType,
    node: HotStuffTreeNode<TPayload>,
    view_number: ViewId,
    signature: Option<Signature>,
}

impl<TPayload: Payload> QuorumCertificate<TPayload> {
    pub fn new(message_type: HotStuffMessageType, view_number: ViewId, node: HotStuffTreeNode<TPayload>) -> Self {
        Self {
            message_type,
            node,
            view_number,
            signature: None,
        }
    }

    pub fn genesis(payload: TPayload) -> Self {
        Self {
            message_type: HotStuffMessageType::Genesis,
            node: HotStuffTreeNode::genesis(payload),
            view_number: 0.into(),
            signature: None,
        }
    }

    pub fn node(&self) -> &HotStuffTreeNode<TPayload> {
        &self.node
    }

    pub fn view_number(&self) -> ViewId {
        self.view_number
    }

    pub fn combine_sig(&mut self, partial_sig: &Signature) {
        self.signature = match &self.signature {
            None => Some(partial_sig.clone()),
            Some(s) => Some(s.combine(partial_sig)),
        };
    }
}
