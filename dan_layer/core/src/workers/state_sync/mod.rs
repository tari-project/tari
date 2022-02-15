//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod error;
pub use error::StateSyncError;
use log::*;
use rand::{rngs::OsRng, seq::SliceRandom};
use tari_common_types::types::PublicKey;
use tari_utilities::hex::Hex;

use crate::{
    models::CheckpointOutput,
    services::{ValidatorNodeClientFactory, ValidatorNodeRpcClient},
    storage::{
        state::{StateDb, StateDbBackendAdapter, StateDbUnitOfWork, StateDbUnitOfWorkReader},
        StorageError,
    },
};

const LOG_TARGET: &str = "tari::dan::workers::state_sync";

pub struct StateSynchronizer<'a, TStateDbBackendAdapter, TValidatorNodeClientFactory: ValidatorNodeClientFactory> {
    last_checkpoint: &'a CheckpointOutput,
    state_db: &'a mut StateDb<TStateDbBackendAdapter>,
    validator_node_client_factory: &'a TValidatorNodeClientFactory,
    our_address: &'a TValidatorNodeClientFactory::Addr,
}

impl<'a, TStateDbBackendAdapter, TValidatorNodeClientFactory>
    StateSynchronizer<'a, TStateDbBackendAdapter, TValidatorNodeClientFactory>
where
    TStateDbBackendAdapter: StateDbBackendAdapter,
    TValidatorNodeClientFactory: ValidatorNodeClientFactory<Addr = PublicKey>,
{
    pub fn new(
        last_checkpoint: &'a CheckpointOutput,
        state_db: &'a mut StateDb<TStateDbBackendAdapter>,
        validator_node_client_factory: &'a TValidatorNodeClientFactory,
        our_address: &'a TValidatorNodeClientFactory::Addr,
    ) -> Self {
        Self {
            last_checkpoint,
            state_db,
            validator_node_client_factory,
            our_address,
        }
    }

    pub async fn sync(&self) -> Result<(), StateSyncError> {
        let mut committee = self
            .last_checkpoint
            .committee
            .iter()
            .filter(|address| *self.our_address != **address)
            .collect::<Vec<_>>();

        if committee.is_empty() {
            return Err(StateSyncError::NoOtherCommitteeMembersToSync);
        }

        committee.shuffle(&mut OsRng);

        for member in committee {
            match self.try_sync_from(member).await {
                Ok(_) => {
                    info!(target: LOG_TARGET, "Sync complete from committee member {}", member);
                    break;
                },
                Err(err) => {
                    error!(target: LOG_TARGET, "Error syncing from {}: {}", member, err);
                    continue;
                },
            }
        }

        Ok(())
    }

    async fn try_sync_from(&self, member: &TValidatorNodeClientFactory::Addr) -> Result<(), StateSyncError> {
        info!(
            target: LOG_TARGET,
            "Attempting to sync asset '{}' from peer '{}'", self.last_checkpoint.parent_public_key, member
        );
        let mut client = self.validator_node_client_factory.create_client(member);
        let tip_node = client
            .get_tip_node(&self.last_checkpoint.parent_public_key)
            .await?
            .ok_or(StateSyncError::RemotePeerDoesNotHaveTipNode)?;

        // TODO: should rather download the op logs for a checkpoint and reply over initial/current state
        let state_schemas = client
            .get_sidechain_state(&self.last_checkpoint.parent_public_key)
            .await?;

        let mut uow = self.state_db.new_unit_of_work(tip_node.height() as u64);

        for schema in state_schemas {
            let name = schema.name;
            for item in schema.items {
                debug!(
                    target: LOG_TARGET,
                    "Adding schema={}, key={}, value={}",
                    name,
                    item.key.to_hex(),
                    item.value.to_hex()
                );
                uow.set_value(name.clone(), item.key, item.value)?;
            }
        }
        // TODO: Check merkle root before commit

        uow.clear_all_state().map_err(StorageError::from)?;
        uow.commit().map_err(StorageError::from)?;

        let merkle_root = uow.calculate_root()?;
        if self.last_checkpoint.merkle_root.as_slice() != merkle_root.as_bytes() {
            return Err(StateSyncError::InvalidStateMerkleRoot);
        }

        Ok(())
    }
}
