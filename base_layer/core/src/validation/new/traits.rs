// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::Arc;

use tari_common_types::types::BlindingFactor;

use crate::{
    blocks::{BlockHeader, ChainHeader},
    transactions::{
        aggregated_body::AggregateBody,
        transaction_components::{TransactionKernel, TransactionOutput},
    },
    validation::ValidationError,
};

#[allow(dead_code)]
pub struct InternallyValidHeader(pub Arc<BlockHeader>);

pub trait InternalConsistencyHeaderValidator {
    /// Validates a header in isolation, i.e. without looking at previous headers
    fn validate(&self, header: &BlockHeader) -> Result<InternallyValidHeader, ValidationError>;
}

pub trait ChainLinkedHeaderValidator {
    /// Takes an (internally) valid header and validates it in context of previous headers in the chain
    fn validate(
        &self,
        header: &InternallyValidHeader, // ... state from the db needed for validation...
    ) -> Result<ChainHeader, ValidationError>;
}

pub trait InternalConsistencyOutputValidator {
    fn validate(&self, output: &TransactionOutput) -> Result<(), ValidationError>;
}

pub trait InternalConsistencyKernelValidator {
    fn validate(&self, kernel: &TransactionKernel) -> Result<(), ValidationError>;
}

pub trait InternalConsistencyAggregateBodyValidator {
    fn validate(
        &self,
        body: AggregateBody,
        offset: BlindingFactor,
        script_offset: BlindingFactor,
    ) -> Result<(), ValidationError>;
}

pub trait ChainLinkedAggregateBodyValidator {
    fn validate(
        &self,
        body: AggregateBody,
        header: ChainHeader, // .... state or db needed to validate....
    ) -> Result<(), ValidationError>;
}
