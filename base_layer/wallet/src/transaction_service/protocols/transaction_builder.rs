pub struct TransactionBuilder {
    consensus_constants: &ConsensusConstants,
    key_manager: TKeyManagerInterface,
    parameters: TransactionParameters,
}

pub struct TransactionParameters {
    pub fee_per_gram: u64,
    pub recipient_script: Script,
    pub recipient_output_features: OutputFeatures,
    pub recipient_covenant: Covenant,
    pub recipient_minimum_value_promise: Option<u64>,
    pub amount: u64,
    pub recipient_address: TariAddress,
    pub sender_address: TariAddress,
    pub memo_field: Option<MemoField>,
    pub prevent_fee_gt_amount: bool,
    pub lock_height: Option<u64>,
    pub kernel_features: KernelFeatures,
    pub tx_id: TxId,
}

impl TransactionBuilder {
    pub fn new<TKeyManagerInterface>(
        consensus_constants: &ConsensusConstants,
        key_manager: TKeyManagerInterface,
        parameters: TransactionParameters,
    ) -> Self {
        TransactionBuilder {
            consensus_constants,
            key_manager,
            parameters,
        }
    }
}
