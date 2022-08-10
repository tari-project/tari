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

use std::{borrow::Borrow, cell::RefCell};

use tari_template_abi::{decode, CallInfo};

use crate::{
    abi_context::AbiContext,
    models::{Contract, Package},
};

thread_local! {
    static CONTEXT: RefCell<Option<AbiContext>> = RefCell::new(None);
}

pub fn set_context_from_call_info(call_info: &CallInfo) {
    let abi_context = decode(&call_info.abi_context).expect("Failed to decode ABI context");
    with_context(|ctx| {
        *ctx = Some(abi_context);
    });
}

pub fn with_context<R, F: FnOnce(&mut Option<AbiContext>) -> R>(f: F) -> R {
    CONTEXT.borrow().with(|c| f(&mut c.borrow_mut()))
}

pub fn get_context() -> Context {
    Context
}

#[derive(Debug, Default)]
pub struct Context;
impl Context {
    pub fn package(&self) -> Package {
        with_context(|ctx| ctx.as_ref().unwrap().package.clone())
    }

    pub fn contract(&self) -> Contract {
        with_context(|ctx| ctx.as_ref().unwrap().contract.clone())
    }
}
