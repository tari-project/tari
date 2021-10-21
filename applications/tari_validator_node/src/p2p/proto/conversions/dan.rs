//  Copyright 2021, The Tari Project
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
    dan_layer::models::{
        HotStuffMessage,
        HotStuffMessageType,
        HotStuffTreeNode,
        Instruction,
        InstructionSet,
        QuorumCertificate,
        Signature,
        TokenId,
        TreeNodeHash,
        ViewId,
    },
    p2p::proto::dan as dan_proto,
    types::{create_com_sig_from_bytes, PublicKey},
};
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::ByteArray;

impl From<&HotStuffMessage<InstructionSet>> for dan_proto::HotStuffMessage {
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

impl From<&HotStuffTreeNode<InstructionSet>> for dan_proto::HotStuffTreeNode {
    fn from(source: &HotStuffTreeNode<InstructionSet>) -> Self {
        Self {
            parent: Vec::from(source.parent().as_bytes()),
            payload: Some(source.payload().into()),
        }
    }
}

impl From<&QuorumCertificate<InstructionSet>> for dan_proto::QuorumCertificate {
    fn from(source: &QuorumCertificate<InstructionSet>) -> Self {
        Self {
            message_type: source.message_type().as_u8() as i32,
            node: Some(source.node().into()),
            view_number: source.view_number().as_u64(),
            signature: source.signature().map(|s| s.into()),
        }
    }
}

impl From<&Signature> for dan_proto::Signature {
    fn from(_s: &Signature) -> Self {
        Self {}
    }
}

impl From<&InstructionSet> for dan_proto::InstructionSet {
    fn from(source: &InstructionSet) -> Self {
        Self {
            instructions: source.instructions().iter().map(|i| i.into()).collect(),
        }
    }
}

impl From<&Instruction> for dan_proto::Instruction {
    fn from(source: &Instruction) -> Self {
        Self {
            asset_id: Vec::from(source.asset_id().as_bytes()),
            method: source.method().to_string(),
            args: Vec::from(source.args()),
            from: Vec::from(source.from_owner().as_bytes()),
            signature: vec![], // com_sig_to_bytes(source.signature()),
        }
    }
}

impl TryFrom<dan_proto::HotStuffMessage> for HotStuffMessage<InstructionSet> {
    type Error = String;

    fn try_from(value: dan_proto::HotStuffMessage) -> Result<Self, Self::Error> {
        Ok(Self::new(
            ViewId(value.view_number),
            HotStuffMessageType::try_from(value.message_type as u8)?,
            value.justify.map(|j| j.try_into()).transpose()?,
            value.node.map(|n| n.try_into()).transpose()?,
            value.partial_sig.map(|p| p.try_into()).transpose()?,
        ))
    }
}

impl TryFrom<dan_proto::QuorumCertificate> for QuorumCertificate<InstructionSet> {
    type Error = String;

    fn try_from(value: dan_proto::QuorumCertificate) -> Result<Self, Self::Error> {
        Ok(Self::new(
            HotStuffMessageType::try_from(value.message_type as u8)?,
            ViewId(value.view_number),
            value
                .node
                .map(|n| n.try_into())
                .transpose()?
                .ok_or_else(|| "node not provided on Quorum Certificate".to_string())?,
            value.signature.map(|s| s.try_into()).transpose()?,
        ))
    }
}

impl TryFrom<dan_proto::HotStuffTreeNode> for HotStuffTreeNode<InstructionSet> {
    type Error = String;

    fn try_from(value: dan_proto::HotStuffTreeNode) -> Result<Self, Self::Error> {
        if value.parent.is_empty() {
            return Err("parent not provided".to_string());
        }
        Ok(Self::new(
            TreeNodeHash(value.parent),
            value
                .payload
                .map(|p| p.try_into())
                .transpose()?
                .ok_or_else(|| "payload not provided".to_string())?,
        ))
    }
}

impl TryFrom<dan_proto::Signature> for Signature {
    type Error = String;

    fn try_from(_value: dan_proto::Signature) -> Result<Self, Self::Error> {
        Ok(Self {})
    }
}

impl TryFrom<dan_proto::InstructionSet> for InstructionSet {
    type Error = String;

    fn try_from(value: dan_proto::InstructionSet) -> Result<Self, Self::Error> {
        let instructions: Vec<Instruction> = value
            .instructions
            .into_iter()
            .map(|i| i.try_into())
            .collect::<Result<_, String>>()?;
        Ok(Self::from_slice(&instructions))
    }
}

impl TryFrom<dan_proto::Instruction> for Instruction {
    type Error = String;

    fn try_from(value: dan_proto::Instruction) -> Result<Self, Self::Error> {
        Ok(Self::new(
            PublicKey::from_bytes(&value.asset_id)
                .map_err(|e| format!("asset_id was not a valid public key: {}", e))?,
            value.method,
            value.args,
            TokenId(value.from),
            create_com_sig_from_bytes(&value.signature)
                .map_err(|e| format!("Could not convert signature bytes to comsig: {}", e))?,
        ))
    }
}
