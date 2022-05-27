// Copyright 2022. The Tari Project
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

use serde::{Deserialize, Serialize};
use tari_common_types::types::{FixedHash, PublicKey};
use tari_core::transactions::transaction_components::{
    vec_into_fixed_string,
    ContractDefinition,
    ContractSpecification,
    FunctionRef,
    PublicFunction,
};
use tari_utilities::hex::Hex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractDefinitionFileFormat {
    pub contract_name: String,
    pub contract_issuer: PublicKey,
    pub contract_spec: ContractSpecificationFileFormat,
}

impl From<ContractDefinitionFileFormat> for ContractDefinition {
    fn from(value: ContractDefinitionFileFormat) -> Self {
        let contract_name = value.contract_name.into_bytes();
        let contract_issuer = value.contract_issuer;
        let contract_spec = value.contract_spec.into();

        Self::new(contract_name, contract_issuer, contract_spec)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractSpecificationFileFormat {
    pub runtime: String,
    pub public_functions: Vec<PublicFunctionFileFormat>,
}

impl From<ContractSpecificationFileFormat> for ContractSpecification {
    fn from(value: ContractSpecificationFileFormat) -> Self {
        Self {
            runtime: vec_into_fixed_string(value.runtime.into_bytes()),
            public_functions: value.public_functions.into_iter().map(|f| f.into()).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicFunctionFileFormat {
    pub name: String,
    pub function: FunctionRefFileFormat,
}

impl From<PublicFunctionFileFormat> for PublicFunction {
    fn from(value: PublicFunctionFileFormat) -> Self {
        Self {
            name: vec_into_fixed_string(value.name.into_bytes()),
            function: value.function.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionRefFileFormat {
    pub template_id: String,
    pub function_id: u16,
}

impl From<FunctionRefFileFormat> for FunctionRef {
    fn from(value: FunctionRefFileFormat) -> Self {
        Self {
            template_id: FixedHash::from_hex(&value.template_id).unwrap(),
            function_id: value.function_id,
        }
    }
}
