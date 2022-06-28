// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod error;
mod flow_factory;
mod flow_instance;
pub mod workers;

use std::any::Any;

pub use error::FlowEngineError;
pub use flow_factory::FlowFactory;
pub use flow_instance::FlowInstance;
use tari_common_types::types::PublicKey;

#[derive(Clone, Debug)]
pub enum ArgValue {
    String(String),
    Byte(u8),
    PublicKey(PublicKey),
    Uint(u64),
}

impl ArgValue {
    pub fn into_any(self) -> Box<dyn Any> {
        match self {
            ArgValue::String(s) => Box::new(s),
            ArgValue::Byte(b) => Box::new(b),
            ArgValue::PublicKey(k) => Box::new(k),
            ArgValue::Uint(u) => Box::new(u),
        }
    }
}
