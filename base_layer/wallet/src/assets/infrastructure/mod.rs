
mod asset_manager_service;
pub use asset_manager_service::AssetManagerService;
use crate::assets::Asset;

use tari_core::transactions::transaction::Transaction;
use tari_core::transactions::transaction_protocol::TxId;
use tari_core::transactions::types::{PublicKey, Commitment};

pub mod initializer;


pub enum AssetManagerRequest {
    ListOwned{},
    GetOwnedAsset{ public_key: PublicKey},
    CreateRegistrationTransaction{name: String},
    CreateMintingTransaction{asset_public_key: Box<PublicKey>, asset_owner_commitment: Box<Commitment>, unique_ids: Vec<Vec<u8>>}
}

pub enum AssetManagerResponse {
    ListOwned{ assets : Vec<Asset>},
    GetOwnedAsset{ asset: Box<Asset>},
    CreateRegistrationTransaction{transaction: Box<Transaction>, tx_id: TxId},
    CreateMintingTransaction{transaction: Box<Transaction>, tx_id: TxId}
}
