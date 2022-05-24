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

use integer_encoding::VarInt;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    array::copy_into_fixed_array_lossy,
    types::{FixedHash, PublicKey},
};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeVec};

// Maximum number of functions allowed in a contract specification
const MAX_FUNCTIONS: usize = u16::MAX as usize;

// Fixed lenght of all string fields in the contract definition
const STR_LEN: usize = 32;

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractDefinitionFeatures {
    pub contract_id: FixedHash,
    pub contract_name: Vec<u8>, // TODO: check length
    pub contract_issuer: PublicKey,
    pub contract_spec: ContractSpecification,
}

impl ConsensusEncoding for ContractDefinitionFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.contract_id.consensus_encode(writer)?;
        copy_into_fixed_array_lossy::<_, STR_LEN>(&self.contract_name).consensus_encode(writer)?;
        self.contract_issuer.consensus_encode(writer)?;
        self.contract_spec.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractDefinitionFeatures {
    fn consensus_encode_exact_size(&self) -> usize {
        self.contract_id.consensus_encode_exact_size() +
            STR_LEN +
            self.contract_issuer.consensus_encode_exact_size() +
            self.contract_spec.consensus_encode_exact_size()
    }
}

impl ConsensusDecoding for ContractDefinitionFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let contract_id = FixedHash::consensus_decode(reader)?;
        let contract_name: Vec<u8> = <[u8; STR_LEN] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        let contract_issuer = PublicKey::consensus_decode(reader)?;
        let contract_spec = ContractSpecification::consensus_decode(reader)?;

        Ok(Self {
            contract_id,
            contract_name,
            contract_issuer,
            contract_spec,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractSpecification {
    pub runtime: Vec<u8>, // TODO: make it String size 32
    pub public_functions: Vec<PublicFunction>,
}

impl ConsensusEncoding for ContractSpecification {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        copy_into_fixed_array_lossy::<_, STR_LEN>(&self.runtime).consensus_encode(writer)?;
        self.public_functions.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractSpecification {
    fn consensus_encode_exact_size(&self) -> usize {
        let public_function_size = match self.public_functions.first() {
            None => 0,
            Some(function) => function.consensus_encode_exact_size(),
        };

        STR_LEN + self.public_functions.len().required_space() + self.public_functions.len() * public_function_size
    }
}

impl ConsensusDecoding for ContractSpecification {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let runtime: Vec<u8> = <[u8; STR_LEN] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        let public_functions = MaxSizeVec::<PublicFunction, MAX_FUNCTIONS>::consensus_decode(reader)?.into_vec();

        Ok(Self {
            runtime,
            public_functions,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct PublicFunction {
    pub name: Vec<u8>,
    pub function: FunctionRef,
}

impl ConsensusEncoding for PublicFunction {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        copy_into_fixed_array_lossy::<_, STR_LEN>(&self.name).consensus_encode(writer)?;
        self.function.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for PublicFunction {
    fn consensus_encode_exact_size(&self) -> usize {
        STR_LEN + self.function.consensus_encode_exact_size()
    }
}

impl ConsensusDecoding for PublicFunction {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let name: Vec<u8> = <[u8; STR_LEN] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        let function = FunctionRef::consensus_decode(reader)?;

        Ok(Self { name, function })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
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
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = ContractDefinitionFeatures {
            contract_id: [1u8; 32],
            contract_name: str_to_padded_vec("contract_name"),
            contract_issuer: PublicKey::default(),
            contract_spec: ContractSpecification {
                runtime: str_to_padded_vec("runtime"),
                public_functions: vec![
                    PublicFunction {
                        name: str_to_padded_vec("foo"),
                        function: FunctionRef {
                            template_id: [1u8; 32],
                            function_id: 0_u16,
                        },
                    },
                    PublicFunction {
                        name: str_to_padded_vec("bar"),
                        function: FunctionRef {
                            template_id: [1u8; 32],
                            function_id: 1_u16,
                        },
                    },
                ],
            },
        };

        check_consensus_encoding_correctness(subject).unwrap();
    }

    fn str_to_padded_vec(s: &str) -> Vec<u8> {
        let mut array_tmp = [0u8; 32];
        array_tmp[..s.len()].copy_from_slice(s.as_bytes());
        array_tmp.to_vec()
    }
}
