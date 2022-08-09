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

use tari_template_abi::{Decode, Encode};
use tari_template_lib::Hash;

pub type ResourceAddress = Hash;

#[derive(Debug, Clone, Encode, Decode, serde::Deserialize)]
pub enum Resource {
    Coin {
        address: ResourceAddress,
        // type_descriptor: TypeDescriptor,
        amount: u64,
    },
    Token {
        address: ResourceAddress,
        // type_descriptor: TypeDescriptor,
        token_ids: Vec<u64>,
    },
}

pub trait ResourceTypeDescriptor {
    fn type_descriptor(&self) -> TypeDescriptor;
}

// The thinking here, that a resource address + a "local" type id together can used to validate type safety of the
// resources at runtime. The local type id can be defined as a unique id within the scope of the contract. We'll have to
// get further to see if this can work or is even needed.
#[derive(Debug, Clone, Encode, Decode, serde::Deserialize)]
pub struct TypeDescriptor {
    type_id: u16,
}
