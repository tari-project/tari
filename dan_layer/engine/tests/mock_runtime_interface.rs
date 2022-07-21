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

use std::sync::{atomic::AtomicU32, Arc};

use tari_dan_common_types::Hash;
use tari_dan_engine::{
    models::{Component, ComponentId},
    runtime::{RuntimeError, RuntimeInterface},
};
use tari_template_abi::LogLevel;

#[derive(Debug, Clone, Default)]
pub struct MockRuntimeInterface {
    ids: Arc<AtomicU32>,
}

impl MockRuntimeInterface {
    pub fn new() -> Self {
        Self {
            ids: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn next_id(&self) -> u32 {
        self.ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

impl RuntimeInterface for MockRuntimeInterface {
    fn emit_log(&self, level: LogLevel, message: &str) {
        let level = match level {
            LogLevel::Error => log::Level::Error,
            LogLevel::Warn => log::Level::Warn,
            LogLevel::Info => log::Level::Info,
            LogLevel::Debug => log::Level::Debug,
        };
        eprintln!("[{:?}] {}", level, message);
        log::log!(target: "tari::dan::engine::runtime", level, "{}", message);
    }

    fn create_component(&self, _new_component: Component) -> Result<ComponentId, RuntimeError> {
        Ok((Hash::default(), self.next_id()))
    }
}
