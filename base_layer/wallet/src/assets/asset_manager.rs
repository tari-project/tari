use crate::error::WalletError;
use crate::assets::Asset;
use crate::output_manager_service::storage::database::{OutputManagerDatabase, OutputManagerBackend};
use tari_core::transactions::transaction::{OutputFlags, Transaction, OutputFeatures, UnblindedOutput};
use crate::error::WalletError::WalletRecoveryError;
use crate::output_manager_service::handle::OutputManagerHandle;
use crate::transaction_service::handle::TransactionServiceHandle;

pub struct AssetManager<T:OutputManagerBackend + 'static>  {
     output_database : OutputManagerDatabase<T>,
     output_manager: OutputManagerHandle,
    transaction_service: TransactionServiceHandle
 }
 impl<T:OutputManagerBackend + 'static> AssetManager<T> {
     pub fn new(backend: T, output_manager: OutputManagerHandle, transaction_service: TransactionServiceHandle) -> Self {
         Self{ output_database: OutputManagerDatabase::new(backend), output_manager, transaction_service}
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

     pub async fn create_registration_transaction(&mut self, name: String) -> Result<Transaction, WalletError>{
         let serializer = V1AssetMetadataSerializer{};

         let metadata = AssetMetadata {
             name
         };
         let mut metadata_bin = vec![1u8];
         metadata_bin.extend(serializer.serialize(&metadata).into_iter());
         let output = self.output_manager.create_output_with_features(0.into(), OutputFeatures::custom(OutputFlags::ASSET_REGISTRATION, metadata_bin)).await?;
         let transaction = self.output_manager.create_send_to_self_with_output(0.into(), vec![output], 100.into()).await?;
        Ok(transaction)
     }
}

fn get_deserializer(version: u8) -> impl AssetMetadataDeserializer {
    match version {
        _ => V1AssetMetadataSerializer{}
    }
}

pub trait AssetMetadataDeserializer {
    fn deserialize(&self, metadata: &[u8]) -> AssetMetadata;
}
pub trait AssetMetadataSerializer {
    fn serialize(&self, model: &AssetMetadata) -> Vec<u8> ;
}

pub struct V1AssetMetadataSerializer {

}

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
