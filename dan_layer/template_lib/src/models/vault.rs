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

use crate::models::{Bucket, ResourceAddress};

#[derive(Clone, Debug, Decode, Encode)]
pub struct Vault<T> {
    resource_address: ResourceAddress<T>,
}

impl<T> Vault<T> {
    pub fn new(resource_address: ResourceAddress<T>) -> Self {
        // Call to call_engine will rather be in the ResourceBuilder/VaultBuilder, and the resulting address passed in
        // here. let resource_address = call_engine(OP_RESOURCE_INVOKE, ResourceInvoke {
        //     resource_ref: ResourceRef::Vault,
        //     action: ResourceAction::Create,
        //     args: args![],
        // });

        Self { resource_address }
    }

    pub fn put(&mut self, _bucket: Bucket<T>) {
        // let _ok: () = call_engine(OP_RESOURCE_INVOKE, ResourceInvoke {
        //     resource_ref: ResourceRef::VaultRef(self.resource_address()),
        //     action: ResourceAction::Put,
        //     args: args![bucket],
        // });
        todo!()
    }

    pub fn resource_address(&self) -> ResourceAddress<T> {
        self.resource_address
    }
}
