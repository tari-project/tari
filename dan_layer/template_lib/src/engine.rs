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

use tari_template_abi::{call_engine, decode, encode, Decode, Encode};

use crate::{
    args::{CreateComponentArg, EmitLogArg, GetComponentArg, LogLevel, SetComponentStateArg},
    context::Context,
    get_context,
    models::{Component, ComponentId},
    ops::*,
};

pub fn engine() -> TariEngine {
    // TODO: I expect some thread local state to be included here
    TariEngine::new(get_context())
}

#[derive(Debug, Default)]
pub struct TariEngine {
    context: Context,
}

impl TariEngine {
    fn new(context: Context) -> Self {
        Self { context }
    }

    pub fn instantiate<T: Encode>(&self, template_name: String, initial_state: T) -> ComponentId {
        let encoded_state = encode(&initial_state).unwrap();

        // Call the engine to create a new component
        // TODO: proper component id
        // TODO: what happens if the user wants to return multiple components/types?
        let component_id = call_engine::<_, ComponentId>(OP_CREATE_COMPONENT, &CreateComponentArg {
            contract_address: *self.context.contract().address(),
            module_name: template_name,
            state: encoded_state,
            package_id: *self.context.package().id(),
        });
        component_id.expect("no asset id returned")
    }

    pub fn emit_log<T: Into<String>>(&self, level: LogLevel, msg: T) {
        call_engine::<_, ()>(OP_EMIT_LOG, &EmitLogArg {
            level,
            message: msg.into(),
        });
    }

    /// Get the component state
    pub fn get_component_state<T: Decode>(&self, component_id: ComponentId) -> T {
        let component = call_engine::<_, Component>(OP_GET_COMPONENT, &GetComponentArg { component_id })
            .expect("Component not found");

        decode(&component.state).expect("Failed to decode component state")
    }

    pub fn set_component_state<T: Encode>(&self, component_id: ComponentId, state: T) {
        let state = encode(&state).unwrap();
        call_engine::<_, ()>(OP_SET_COMPONENT_STATE, &SetComponentStateArg { component_id, state });
    }
}
