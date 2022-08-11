//  Copyright 2022. The Tari Project
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

use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, RwLock},
};

use tari_common_types::types::FixedHash;
use tari_template_lib::{
    args::LogLevel,
    models::{Component, ComponentId, ComponentInstance},
};

use crate::{models::Bucket, state_store::StateStoreError};

#[derive(Clone)]
pub struct Runtime {
    tracker: Arc<RwLock<ChangeTracker>>,
    interface: Arc<dyn RuntimeInterface>,
}

impl Runtime {
    pub fn new(engine: Arc<dyn RuntimeInterface>) -> Self {
        Self {
            tracker: Arc::new(RwLock::new(ChangeTracker::default())),
            interface: engine,
        }
    }

    pub fn interface(&self) -> &dyn RuntimeInterface {
        &*self.interface
    }
}

impl Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("tracker", &self.tracker)
            .field("engine", &"dyn RuntimeEngine")
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChangeTracker {
    pub buckets: HashMap<FixedHash, Bucket>,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("State DB error: {0}")]
    StateDbError(#[from] anyhow::Error),
    #[error("State storage error: {0}")]
    StateStoreError(#[from] StateStoreError),
    #[error("Component not found with id '{id}'")]
    ComponentNotFound { id: ComponentId },
}

pub trait RuntimeInterface: Send + Sync {
    fn emit_log(&self, level: LogLevel, message: &str);
    fn create_component(&self, component: Component) -> Result<ComponentId, RuntimeError>;
    fn get_component(&self, component_id: &ComponentId) -> Result<ComponentInstance, RuntimeError>;
    fn set_component_state(&self, component_id: &ComponentId, state: Vec<u8>) -> Result<(), RuntimeError>;
}
