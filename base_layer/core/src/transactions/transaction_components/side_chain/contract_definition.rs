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

use std::io::{Error, Read, Write};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, FixedHash, PublicKey};

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, ConsensusHashWriter, MaxSizeVec},
    transactions::transaction_components::FixedString,
};

// Maximum number of functions allowed in a contract specification
const MAX_FUNCTIONS: usize = u16::MAX as usize;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq, Hash)]
pub struct ContractDefinition {
    pub contract_name: FixedString,
    pub contract_issuer: PublicKey,
    pub contract_spec: ContractSpecification,
}

impl ContractDefinition {
    pub fn new(contract_name: FixedString, contract_issuer: PublicKey, contract_spec: ContractSpecification) -> Self {
        Self {
            contract_name,
            contract_issuer,
            contract_spec,
        }
    }

    pub fn calculate_contract_id(&self, commitment: &Commitment) -> FixedHash {
        ConsensusHashWriter::default()
            .chain(commitment)
            .chain(self)
            .finalize()
            .into()
    }
}

impl ConsensusEncoding for ContractDefinition {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.contract_name.consensus_encode(writer)?;
        self.contract_issuer.consensus_encode(writer)?;
        self.contract_spec.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractDefinition {
    fn consensus_encode_exact_size(&self) -> usize {
        self.contract_name.consensus_encode_exact_size() +
            self.contract_issuer.consensus_encode_exact_size() +
            self.contract_spec.consensus_encode_exact_size()
    }
}

impl ConsensusDecoding for ContractDefinition {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let contract_name = FixedString::consensus_decode(reader)?;
        let contract_issuer = PublicKey::consensus_decode(reader)?;
        let contract_spec = ContractSpecification::consensus_decode(reader)?;

        Ok(Self {
            contract_name,
            contract_issuer,
            contract_spec,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq, Hash)]
pub struct ContractSpecification {
    pub runtime: FixedString,
    pub public_functions: Vec<PublicFunction>,
}

impl ConsensusEncoding for ContractSpecification {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.runtime.consensus_encode(writer)?;
        self.public_functions.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractSpecification {}

impl ConsensusDecoding for ContractSpecification {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let runtime = FixedString::consensus_decode(reader)?;
        let public_functions = MaxSizeVec::<PublicFunction, MAX_FUNCTIONS>::consensus_decode(reader)?.into_vec();

        Ok(Self {
            runtime,
            public_functions,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq, Hash)]
pub struct PublicFunction {
    pub name: FixedString,
    pub function: FunctionRef,
}

impl ConsensusEncoding for PublicFunction {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.name.consensus_encode(writer)?;
        self.function.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for PublicFunction {
    fn consensus_encode_exact_size(&self) -> usize {
        self.name.consensus_encode_exact_size() + self.function.consensus_encode_exact_size()
    }
}

impl ConsensusDecoding for PublicFunction {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let name = FixedString::consensus_decode(reader)?;
        let function = FunctionRef::consensus_decode(reader)?;

        Ok(Self { name, function })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq, Hash)]
pub struct FunctionRef {
    pub template_id: FixedHash,
    pub function_id: u16,
}

impl ConsensusEncoding for FunctionRef {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.template_id.consensus_encode(writer)?;
        self.function_id.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for FunctionRef {
    fn consensus_encode_exact_size(&self) -> usize {
        self.template_id.consensus_encode_exact_size() + self.function_id.consensus_encode_exact_size()
    }
}

impl ConsensusDecoding for FunctionRef {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let template_id = FixedHash::consensus_decode(reader)?;
        let function_id = u16::consensus_decode(reader)?;

        Ok(Self {
            template_id,
            function_id,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        consensus::check_consensus_encoding_correctness,
        transactions::transaction_components::bytes_into_fixed_string,
    };

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let contract_name = bytes_into_fixed_string("contract_name");
        let contract_issuer = PublicKey::default();
        let contract_spec = ContractSpecification {
            runtime: bytes_into_fixed_string("runtime value"),
            public_functions: vec![
                PublicFunction {
                    name: bytes_into_fixed_string("foo"),
                    function: FunctionRef {
                        template_id: FixedHash::zero(),
                        function_id: 0_u16,
                    },
                },
                PublicFunction {
                    name: bytes_into_fixed_string("bar"),
                    function: FunctionRef {
                        template_id: FixedHash::zero(),
                        function_id: 1_u16,
                    },
                },
            ],
        };

        let contract_definition = ContractDefinition::new(contract_name, contract_issuer, contract_spec);

        check_consensus_encoding_correctness(contract_definition).unwrap();
    }
}
