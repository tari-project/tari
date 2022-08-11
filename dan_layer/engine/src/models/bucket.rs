// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::Deserialize;
use tari_template_abi::{Decode, Encode};

use crate::models::resource::{Resource, ResourceAddress};

#[derive(Debug, Clone, Encode, Decode, Deserialize)]
pub struct Bucket {
    resource: Resource,
}

impl Bucket {
    pub fn for_coin(address: ResourceAddress, amount: u64) -> Self {
        Self {
            resource: Resource::Coin { address, amount },
        }
    }

    pub fn for_token(address: ResourceAddress, token_ids: Vec<u64>) -> Self {
        Self {
            resource: Resource::Token { address, token_ids },
        }
    }

    pub fn amount(&self) -> u64 {
        match self.resource {
            Resource::Coin { ref amount, .. } => *amount,
            Resource::Token { ref token_ids, .. } => token_ids.len() as u64,
        }
    }

    pub fn resource_address(&self) -> ResourceAddress {
        match self.resource {
            Resource::Coin { address, .. } => address,
            Resource::Token { address, .. } => address,
        }
    }

    pub fn token_ids(&self) -> Option<&[u64]> {
        match self.resource {
            Resource::Coin { .. } => None,
            Resource::Token { ref token_ids, .. } => Some(token_ids),
        }
    }
}
