//  Copyright 2019 The Tari Project
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

use super::{DispatchError, DispatchResolver, Dispatcher};
use crate::{
    dispatcher::{DispatchableKey, HandlerError},
    message::{DomainMessageContext, MessageHeader},
};
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, marker::PhantomData};

/// Domain-level dispatch resolver
pub struct DomainDispatchResolver<MType>(PhantomData<MType>);

impl<MType> DomainDispatchResolver<MType> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<MType> DispatchResolver<MType, DomainMessageContext> for DomainDispatchResolver<MType>
where MType: DeserializeOwned + Serialize
{
    fn resolve(&self, msg: &DomainMessageContext) -> Result<MType, DispatchError> {
        let header: MessageHeader<MType> = msg.message.to_header().map_err(DispatchError::resolve_failed())?;

        Ok(header.message_type)
    }
}

/// Dispatcher format for domain level dispatching to handlers
pub type DomainMessageDispatcher<MType, E = HandlerError> =
    Dispatcher<MType, DomainMessageContext, DomainDispatchResolver<MType>, E>;

impl<MType, E> Default for DomainMessageDispatcher<MType, E>
where
    MType: DispatchableKey,
    MType: DeserializeOwned + Serialize,
    E: Error,
{
    fn default() -> Self {
        DomainMessageDispatcher::<MType, E>::new(DomainDispatchResolver::<MType>::new())
    }
}
