
mod asset_manager_service;
pub use asset_manager_service::AssetManagerService;
use crate::assets::Asset;
pub mod initializer;


pub enum AssetManagerRequest {
    ListOwned{}
}

pub enum AssetManagerResponse {
    ListOwned{ assets : Vec<Asset>}
}
