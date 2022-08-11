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

use std::sync::{atomic::AtomicU32, Arc, RwLock};

use digest::Digest;
use tari_dan_engine::{
    crypto,
    runtime::{RuntimeError, RuntimeInterface},
    state_store::{memory::MemoryStateStore, AtomicDb, StateReader, StateWriter},
};
use tari_template_lib::{
    args::LogLevel,
    models::{Component, ComponentId, ComponentInstance},
};

#[derive(Debug, Clone, Default)]
pub struct MockRuntimeInterface {
    ids: Arc<AtomicU32>,
    state: MemoryStateStore,
    calls: Arc<RwLock<Vec<&'static str>>>,
}

impl MockRuntimeInterface {
    pub fn new() -> Self {
        Self {
            ids: Arc::new(AtomicU32::new(0)),
            state: MemoryStateStore::default(),
            calls: Arc::new(RwLock::new(vec![])),
        }
    }

    pub fn state_store(&self) -> MemoryStateStore {
        self.state.clone()
    }

    pub fn next_id(&self) -> u32 {
        self.ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub fn get_calls(&self) -> Vec<&'static str> {
        self.calls.read().unwrap().clone()
    }

    pub fn clear_calls(&self) {
        self.calls.write().unwrap().clear();
    }

    fn add_call(&self, call: &'static str) {
        self.calls.write().unwrap().push(call);
    }
}

impl RuntimeInterface for MockRuntimeInterface {
    fn emit_log(&self, level: LogLevel, message: &str) {
        self.add_call("emit_log");
        let level = match level {
            LogLevel::Error => log::Level::Error,
            LogLevel::Warn => log::Level::Warn,
            LogLevel::Info => log::Level::Info,
            LogLevel::Debug => log::Level::Debug,
        };
        eprintln!("[{:?}] {}", level, message);
        log::log!(target: "tari::dan::engine::runtime", level, "{}", message);
    }

    fn create_component(&self, new_component: Component) -> Result<ComponentId, RuntimeError> {
        self.add_call("create_component");
        let component_id: [u8; 32] = crypto::hasher("component")
            .chain(self.next_id().to_le_bytes())
            .finalize()
            .into();

        let component = ComponentInstance::new(component_id.into(), new_component);
        let mut tx = self.state.write_access().map_err(RuntimeError::StateDbError)?;
        tx.set_state(&component_id, component)?;
        self.state.commit(tx).map_err(RuntimeError::StateDbError)?;

        Ok(component_id.into())
    }

    fn get_component(&self, component_id: &ComponentId) -> Result<ComponentInstance, RuntimeError> {
        self.add_call("get_component");
        let component = self
            .state
            .read_access()
            .map_err(RuntimeError::StateDbError)?
            .get_state(component_id)?
            .ok_or(RuntimeError::ComponentNotFound { id: *component_id })?;
        Ok(component)
    }

    fn set_component_state(&self, component_id: &ComponentId, state: Vec<u8>) -> Result<(), RuntimeError> {
        self.add_call("set_component_state");
        let mut tx = self.state.write_access().map_err(RuntimeError::StateDbError)?;
        let mut component: ComponentInstance = tx
            .get_state(component_id)?
            .ok_or(RuntimeError::ComponentNotFound { id: *component_id })?;
        component.state = state;
        tx.set_state(&component_id, component)?;
        self.state.commit(tx).map_err(RuntimeError::StateDbError)?;

        Ok(())
    }
}
