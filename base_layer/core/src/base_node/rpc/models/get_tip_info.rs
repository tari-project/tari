// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TipInfoResponse {
    pub metadata: Option<tari_common_types::chain_metadata::ChainMetadata>,
    pub is_synced: bool,
}

impl TryFrom<crate::proto::base_node::TipInfoResponse> for TipInfoResponse {
    type Error = String;

    fn try_from(proto_value: crate::proto::base_node::TipInfoResponse) -> Result<Self, Self::Error> {
        let chain_metadata = match proto_value.metadata.map(|m| {
            let result: Result<tari_common_types::chain_metadata::ChainMetadata, String> = m.try_into();
            result
        }) {
            Some(result) => Some(result?),
            None => None,
        };
        Ok(Self {
            metadata: chain_metadata,
            is_synced: proto_value.is_synced,
        })
    }
}
