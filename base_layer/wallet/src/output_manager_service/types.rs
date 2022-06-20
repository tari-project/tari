use tari_common_types::transaction::TxId;
use tari_core::transactions::{tari_amount::MicroTari, transaction_components::Transaction};

use crate::output_manager_service::storage::models::DbUnblindedOutput;

#[derive(Debug, Clone)]
pub struct CoinJoinResult {
    pub tx_id: TxId,
    pub transaction: Transaction,
    pub src_outputs: Vec<DbUnblindedOutput>,
    pub computed_fee_amount: MicroTari,
    pub target_amount: MicroTari,
    pub total_expense_amount: MicroTari,
    pub aggregated_amount: MicroTari,
    pub leftover_change_amount: MicroTari,
    pub primary_output: DbUnblindedOutput,
    pub leftover_change_output: DbUnblindedOutput,
}
