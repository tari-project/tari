use crate::error::WalletError;
use crate::assets::Asset;
use crate::output_manager_service::storage::database::{OutputManagerDatabase, OutputManagerBackend};
use tari_core::transactions::transaction::OutputFlags;
use crate::error::WalletError::WalletRecoveryError;

pub struct AssetManager<T:OutputManagerBackend + 'static>  {
     output_database : OutputManagerDatabase<T>
 }
 impl<T:OutputManagerBackend + 'static> AssetManager<T> {
     pub fn new(backend: T) -> Self {
         Self{ output_database: OutputManagerDatabase::new(backend)}
     }

     // TODO: Write test
    pub async fn list_owned(&self) -> Result<Vec<Asset>, WalletError>{
        let outputs = self.output_database.fetch_with_features(OutputFlags::ASSET_REGISTRATION).await.map_err(|err| WalletError::OutputManagerError(err.into()))?;

         let assets = outputs.into_iter().map(|unblinded_output| {
             let version = unblinded_output.unblinded_output.features.metadata[0];

             let deserializer = get_deserializer(version);

             let metadata = deserializer.deserialize(&unblinded_output.unblinded_output.features.metadata[1..]);
             Asset::new(metadata.name)
         }
         ).collect();
         Ok(assets)
    }
}

fn get_deserializer(version: u8) -> impl AssetMetadataDeserializer {
    match version {
        _ => V1AssetMetadataDeserializer{}
    }
}

pub trait AssetMetadataDeserializer {
    fn deserialize(&self, _metadata: &[u8]) -> AssetMetadata {
        return AssetMetadata{
            name: "Big Neon Tickets".to_string()
        }
    }
}


pub struct V1AssetMetadataDeserializer {

}


impl AssetMetadataDeserializer for V1AssetMetadataDeserializer {

}

pub struct AssetMetadata {
    name: String
}
