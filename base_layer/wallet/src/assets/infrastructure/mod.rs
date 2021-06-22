
mod asset_manager_service;
pub use asset_manager_service::AssetManagerService;
use crate::assets::Asset;

use tari_core::transactions::transaction::Transaction;
use tari_core::transactions::transaction_protocol::TxId;

pub mod initializer;


pub enum AssetManagerRequest {
    ListOwned{},
    CreateRegistrationTransaction{name: String}
}

pub enum AssetManagerResponse {
    ListOwned{ assets : Vec<Asset>},
    CreateRegistrationTransaction{transaction: Transaction, tx_id: TxId}
}
