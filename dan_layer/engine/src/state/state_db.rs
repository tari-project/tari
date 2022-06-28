// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::types::FixedHash;

use crate::state::{
    state_db_unit_of_work::{StateDbUnitOfWorkImpl, StateDbUnitOfWorkReader, UnitOfWorkContext},
    StateDbBackendAdapter,
};

pub struct StateDb<TStateDbBackendAdapter> {
    backend_adapter: TStateDbBackendAdapter,
    contract_id: FixedHash,
}

impl<TStateDbBackendAdapter: StateDbBackendAdapter> StateDb<TStateDbBackendAdapter> {
    pub fn new(contract_id: FixedHash, backend_adapter: TStateDbBackendAdapter) -> Self {
        Self {
            backend_adapter,
            contract_id,
        }
    }

    pub fn new_unit_of_work(&self, height: u64) -> StateDbUnitOfWorkImpl<TStateDbBackendAdapter> {
        StateDbUnitOfWorkImpl::new(
            UnitOfWorkContext::new(height, self.contract_id),
            self.backend_adapter.clone(),
        )
    }

    pub fn reader(&self) -> impl StateDbUnitOfWorkReader {
        // TODO: A reader doesnt need the current context, should perhaps make a read-only implementation that the
        //       writable implementation also uses
        StateDbUnitOfWorkImpl::new(
            UnitOfWorkContext::new(0, self.contract_id),
            self.backend_adapter.clone(),
        )
    }
}
