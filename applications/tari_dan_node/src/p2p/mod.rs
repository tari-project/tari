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

use crate::dan_layer::models::{
    HotStuffMessage,
    HotStuffTreeNode,
    Instruction,
    InstructionSet,
    Payload,
    QuorumCertificate,
    Signature,
};

#[allow(clippy::large_enum_variant)]
pub mod dan_p2p {
    include!(concat!(env!("OUT_DIR"), "/tari.dan_p2p.rs"));
}

impl From<&HotStuffMessage<InstructionSet>> for dan_p2p::HotStuffMessage {
    fn from(source: &HotStuffMessage<InstructionSet>) -> Self {
        Self {
            message_type: source.message_type().as_u8() as i32,
            node: source.node().map(|n| n.into()),
            justify: source.justify().map(|j| j.into()),
            partial_sig: source.partial_sig().map(|s| s.into()),
            view_number: source.view_number().as_u64(),
        }
    }
}

impl From<&HotStuffTreeNode<InstructionSet>> for dan_p2p::HotStuffTreeNode {
    fn from(source: &HotStuffTreeNode<InstructionSet>) -> Self {
        Self {
            parent: Vec::from(source.parent().as_bytes()),
            payload: Some(source.payload().into()),
        }
    }
}

impl From<&QuorumCertificate<InstructionSet>> for dan_p2p::QuorumCertificate {
    fn from(source: &QuorumCertificate<InstructionSet>) -> Self {
        Self {
            message_type: source.message_type().as_u8() as i32,
            node: Some(source.node().into()),
            view_number: source.view_number().as_u64(),
            signature: source.signature().map(|s| s.into()),
        }
    }
}

impl From<&Signature> for dan_p2p::Signature {
    fn from(s: &Signature) -> Self {
        Self {}
    }
}

impl From<&InstructionSet> for dan_p2p::InstructionSet {
    fn from(source: &InstructionSet) -> Self {
        Self {
            instructions: source.instructions().iter().map(|i| i.into()).collect(),
        }
    }
}

impl From<&Instruction> for dan_p2p::Instruction {
    fn from(source: &Instruction) -> Self {
        Self {}
    }
}
