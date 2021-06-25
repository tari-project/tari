use crate::error::WalletError;
use crate::assets::Asset;
use crate::output_manager_service::storage::database::{OutputManagerDatabase, OutputManagerBackend};
use tari_core::transactions::transaction::{OutputFlags, Transaction, OutputFeatures};

use crate::output_manager_service::handle::OutputManagerHandle;
use log::*;
use tari_core::transactions::transaction_protocol::TxId;
use tari_core::transactions::types::{PublicKey};
use crate::types::PersistentKeyManager;
use crate::output_manager_service::storage::models::DbUnblindedOutput;

const LOG_TARGET: &str = "wallet::assets::asset_manager";

pub(crate) struct AssetManager<T:OutputManagerBackend + 'static, TPersistentKeyManager: PersistentKeyManager>  {
     output_database : OutputManagerDatabase<T>,
     output_manager: OutputManagerHandle,
    assets_key_manager: TPersistentKeyManager
    // transaction_service: TransactionServiceHandle
 }
 impl<T:OutputManagerBackend + 'static, TPersistentKeyManager: PersistentKeyManager> AssetManager<T, TPersistentKeyManager> {
     pub fn new(backend: T, output_manager: OutputManagerHandle, assets_key_manager: TPersistentKeyManager) -> Self {
         Self{ output_database: OutputManagerDatabase::new(backend), output_manager, assets_key_manager}
     }

     // TODO: Write test
    pub async fn list_owned(&self) -> Result<Vec<Asset>, WalletError>{
        let outputs = self.output_database.fetch_with_features(OutputFlags::ASSET_REGISTRATION).await.map_err(|err| WalletError::OutputManagerError(err.into()))?;

         debug!(target: LOG_TARGET, "Found {} owned outputs that contain assets", outputs.len());
         let assets = outputs.into_iter().map(|unblinded_output| {
            convert_to_asset(unblinded_output)
         }
         ).collect();
         Ok(assets)
    }

     pub async fn get_owned_asset_by_pub_key(&self, public_key: PublicKey) -> Result<Asset, WalletError> {
         let output = self.output_database.fetch_by_features_asset_public_key(public_key).map_err(|err| WalletError::OutputManagerError(err.into()))?;
        Ok(convert_to_asset(output))
     }

     pub async fn create_registration_transaction(&mut self, name: String) -> Result<(TxId, Transaction), WalletError>{
         let serializer = V1AssetMetadataSerializer{};

         let metadata = AssetMetadata {
             name
         };
         let mut metadata_bin = vec![1u8];
         metadata_bin.extend(serializer.serialize(&metadata).into_iter());

         let public_key = self.assets_key_manager.create_and_store_new()?;
         let output = self.output_manager.create_output_with_features(0.into(), OutputFeatures::for_asset_registration(metadata_bin, public_key)).await?;
         debug!(target: LOG_TARGET, "Created output: {:?}", output);
         let (tx_id, transaction) = self.output_manager.create_send_to_self_with_output(0.into(), vec![output], 100.into()).await?;
        Ok((tx_id, transaction))
     }

     pub async fn create_minting_transaction(&mut self, public_key: PublicKey, unique_ids: Vec<Vec<u8>>) -> Result<(TxId, Transaction), WalletError> {
        unimplemented!()
     }
}


fn convert_to_asset(unblinded_output: DbUnblindedOutput) -> Asset {
    if unblinded_output.unblinded_output.features.metadata.is_empty() {
        return Asset::new("<Invalid metadata:empty>".to_string(), unblinded_output.status.to_string());
    }
    let version = unblinded_output.unblinded_output.features.metadata[0];

    let deserializer = get_deserializer(version);

    let metadata = deserializer.deserialize(&unblinded_output.unblinded_output.features.metadata[1..]);
    Asset::new(metadata.name, unblinded_output.status.to_string())
}

fn get_deserializer(_version: u8) -> impl AssetMetadataDeserializer {
    V1AssetMetadataSerializer{}
}

pub trait AssetMetadataDeserializer {
    fn deserialize(&self, metadata: &[u8]) -> AssetMetadata;
}
pub trait AssetMetadataSerializer {
    fn serialize(&self, model: &AssetMetadata) -> Vec<u8> ;
}

pub struct V1AssetMetadataSerializer {

}

// TODO: Replace with proto serializer
impl AssetMetadataDeserializer for V1AssetMetadataSerializer {
    fn deserialize(&self, metadata: &[u8]) -> AssetMetadata {
        AssetMetadata {
            name: String::from_utf8(Vec::from(metadata)).unwrap()
        }
    }
}

    impl AssetMetadataSerializer for V1AssetMetadataSerializer {
        fn serialize(&self, model: &AssetMetadata) -> Vec<u8> {
           model.name.clone().into_bytes()
        }
    }

pub struct AssetMetadata {
    name: String
}
