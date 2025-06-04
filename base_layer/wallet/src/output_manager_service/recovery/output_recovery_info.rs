use tari_common_types::types::CompressedCommitment;
use tari_core::transactions::transaction_components::EncryptedData;
use tari_script::TariScript;

pub struct OutputRecoveryInfo {
    pub output_hash: Vec<u8>,
    pub block_hash: Vec<u8>,
    pub script: TariScript,
    pub commitment: CompressedCommitment,
    pub encrypted_data: EncryptedData,
}