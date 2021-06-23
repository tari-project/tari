
mod asset_manager_service;
pub use asset_manager_service::AssetManagerService;
use crate::assets::Asset;

use tari_core::transactions::transaction::Transaction;
use tari_core::transactions::transaction_protocol::TxId;
use tari_core::transactions::types::PublicKey;

pub mod initializer;


pub enum AssetManagerRequest {
    ListOwned{},
    GetOwnedAsset{ public_key: PublicKey},
    CreateRegistrationTransaction{name: String}
}

pub enum AssetManagerResponse {
    ListOwned{ assets : Vec<Asset>},
    GetOwnedAsset{ asset: Asset},
    CreateRegistrationTransaction{transaction: Transaction, tx_id: TxId}
}
