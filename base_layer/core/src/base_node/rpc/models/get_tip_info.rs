use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TipInfoResponse {
    pub metadata: Option<tari_common_types::chain_metadata::ChainMetadata>,
    pub is_synced: bool,
}


// TODO: continue impl
impl TryFrom<crate::proto::base_node::TipInfoResponse> for TipInfoResponse {
    type Error = ();

    fn try_from(proto_value: crate::proto::base_node::TipInfoResponse) -> Result<Self, Self::Error> {
        let chain_metadata = match proto_value.metadata.map(|m| {
            let result: Result<tari_common_types::chain_metadata::ChainMetadata, > = m.try_into();
        }) {};
        Ok(
            Self {
                metadata: chain_metadata,
                is_synced: proto_value.is_synced,
            }
        )
    }
}