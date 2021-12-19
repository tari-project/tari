//  Copyright 2021. The Tari Project
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

use crate::{
    services::{
        infrastructure_services::NodeAddressable,
        AssetProcessor,
        AssetProxy,
        BaseNodeClient,
        MempoolService,
        ValidatorNodeClientFactory,
    },
    storage::DbFactory,
};

/// A trait to describe a specific configuration of services. This type allows other services to
/// simply reference types.
/// This trait is intended to only include `types` and no methods.
pub trait ServiceSpecification: Clone {
    type Addr: NodeAddressable;
    type DbFactory: DbFactory + Clone + Sync + Send + 'static;
    type BaseNodeClient: BaseNodeClient + Clone + Sync + Send + 'static;
    type ValidatorNodeClientFactory: ValidatorNodeClientFactory<Addr = Self::Addr> + Clone + Sync + Send + 'static;
    type MempoolService: MempoolService + Clone + Sync + Send + 'static;
    type AssetProcessor: AssetProcessor + Clone + Sync + Send + 'static;
    type AssetProxy: AssetProxy + Clone + Sync + Send + 'static;
}
