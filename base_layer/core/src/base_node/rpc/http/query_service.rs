use crate::base_node::rpc::BaseNodeWalletQueryService;
use crate::base_node::state_machine_service::states::StateInfo;
use crate::base_node::StateMachineHandle;
use crate::blocks::BlockHeader;
use crate::chain_storage::async_db::AsyncBlockchainDb;
use crate::chain_storage::{BlockchainBackend, ChainStorageError};
use crate::proto::base_node::TipInfoResponse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to get chain metadata: {0}")]
    FailedToGetChainMetadata(#[from] ChainStorageError),
    #[error("Header not found at height: {height}")]
    HeaderNotFound { height: u64 },
}

pub struct Service<B> {
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
}

impl<B: BlockchainBackend + 'static> Service<B> {
    pub fn new(db: AsyncBlockchainDb<B>, state_machine: StateMachineHandle) -> Self {
        Self {
            db,
            state_machine,
        }
    }

    fn state_machine(&self) -> StateMachineHandle {
        self.state_machine.clone()
    }
}

#[async_trait::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeWalletQueryService for Service<B> {
    type Error = Error;

    async fn get_tip_info(&self) -> Result<TipInfoResponse, Self::Error> {
        let state_machine = self.state_machine();
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match status_watch.borrow().state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let metadata = self
            .db
            .get_chain_metadata()
            .await?
            .into();

        Ok(
            TipInfoResponse {
                metadata: Some(metadata),
                is_synced,
            }
        )
    }

    async fn get_header_by_height(&self, height: u64) -> Result<BlockHeader, Self::Error> {
        Ok(
            self
                .db
                .fetch_header(height)
                .await?
                .ok_or(Error::HeaderNotFound { height })?
        )
    }
}