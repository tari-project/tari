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

use crate::dan_layer::models::{HotStuffMessageType, HotStuffTreeNode, Payload, QuorumCertificate, ViewId};
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct HotStuffMessage<TPayload: Payload> {
    view_number: ViewId,
    message_type: HotStuffMessageType,
    justify: QuorumCertificate<TPayload>,
    node: Option<HotStuffTreeNode<TPayload>>,
}

impl<TPayload: Payload> HotStuffMessage<TPayload> {
    pub fn new_view(prepare_qc: QuorumCertificate<TPayload>, view_number: ViewId) -> Self {
        Self {
            message_type: HotStuffMessageType::NewView,
            view_number,
            justify: prepare_qc,
            node: None,
        }
    }

    pub fn prepare(
        proposal: HotStuffTreeNode<TPayload>,
        high_qc: QuorumCertificate<TPayload>,
        view_number: ViewId,
    ) -> Self {
        Self {
            message_type: HotStuffMessageType::Prepare,
            node: Some(proposal),
            justify: high_qc,
            view_number,
        }
    }

    pub fn view_number(&self) -> ViewId {
        self.view_number
    }

    pub fn node(&self) -> Option<&HotStuffTreeNode<TPayload>> {
        self.node.as_ref()
    }

    pub fn message_type(&self) -> &HotStuffMessageType {
        &self.message_type
    }

    pub fn justify(&self) -> &QuorumCertificate<TPayload> {
        &self.justify
    }

    pub fn matches(&self, message_type: HotStuffMessageType, view_id: ViewId) -> bool {
        // from hotstuf spec
        self.message_type() == &message_type && view_id == self.view_number()
    }
}
