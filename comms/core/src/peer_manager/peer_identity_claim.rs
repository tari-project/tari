//  Copyright 2022. The Taiji Project
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

use std::convert::{TryFrom, TryInto};

use multiaddr::Multiaddr;
use serde_derive::{Deserialize, Serialize};

use crate::{
    peer_manager::{IdentitySignature, PeerFeatures, PeerManagerError, MAX_USER_AGENT_LEN},
    proto::identity::PeerIdentityMsg,
    protocol::ProtocolId,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PeerIdentityClaim {
    pub addresses: Vec<Multiaddr>,
    pub features: PeerFeatures,
    pub signature: IdentitySignature,
    pub unverified_data: Option<PeerIdentityClaimUnverifiedData>,
}

impl PeerIdentityClaim {
    pub fn new(
        addresses: Vec<Multiaddr>,
        features: PeerFeatures,
        signature: IdentitySignature,
        unverified_data: Option<PeerIdentityClaimUnverifiedData>,
    ) -> Self {
        Self {
            addresses,
            features,
            signature,
            unverified_data,
        }
    }

    pub fn supported_protocols(&self) -> Vec<ProtocolId> {
        self.unverified_data
            .as_ref()
            .map(|d| d.supported_protocols.clone())
            .unwrap_or_default()
    }

    pub fn user_agent(&self) -> Option<String> {
        self.unverified_data.as_ref().map(|d| d.user_agent.clone())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PeerIdentityClaimUnverifiedData {
    pub user_agent: String,
    pub supported_protocols: Vec<ProtocolId>,
}

impl TryFrom<PeerIdentityMsg> for PeerIdentityClaim {
    type Error = PeerManagerError;

    fn try_from(value: PeerIdentityMsg) -> Result<Self, Self::Error> {
        let addresses: Vec<Multiaddr> = value
            .addresses
            .iter()
            .map(|addr_bytes| Multiaddr::try_from(addr_bytes.clone()))
            .collect::<Result<_, _>>()
            .map_err(|e| PeerManagerError::MultiaddrError(e.to_string()))?;

        if addresses.is_empty() {
            return Err(PeerManagerError::PeerIdentityNoValidAddresses);
        }
        let features = PeerFeatures::from_bits_truncate(value.features);

        let supported_protocols = value
            .supported_protocols
            .iter()
            .map(|p| bytes::Bytes::from(p.clone()))
            .collect::<Vec<_>>();

        let mut user_agent = value.user_agent;
        user_agent.truncate(MAX_USER_AGENT_LEN);

        if let Some(signature) = value.identity_signature {
            Ok(Self {
                addresses,
                features,
                signature: signature.try_into()?,
                unverified_data: Some(PeerIdentityClaimUnverifiedData {
                    user_agent,
                    supported_protocols,
                }),
            })
        } else {
            Err(PeerManagerError::MissingIdentitySignature)
        }
    }
}
