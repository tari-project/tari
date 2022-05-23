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

use std::io::{Error, Read, Write};

use serde::{Deserialize, Serialize};
use tari_common_types::{array::copy_into_fixed_array_lossy, types::PublicKey};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

// TODO: define a constant for each dynamic sized field
const FIELD_LEN: usize = 32;

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractDefinitionFeatures {
    pub contract_id: Vec<u8>,       // TODO: make it a hash
    pub contract_name: Vec<u8>,     // TODO: check length
    pub contract_issuer: PublicKey, // TODO: make it a pubkey
    pub contract_spec: ContractSpecification,
}

impl ConsensusEncoding for ContractDefinitionFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        copy_into_fixed_array_lossy::<_, 32>(&self.contract_id).consensus_encode(writer)?;
        copy_into_fixed_array_lossy::<_, 32>(&self.contract_name).consensus_encode(writer)?;
        self.contract_issuer.consensus_encode(writer)?;
        self.contract_spec.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractDefinitionFeatures {
    fn consensus_encode_exact_size(&self) -> usize {
        32 * 4
    }
}

impl ConsensusDecoding for ContractDefinitionFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let contract_id: Vec<u8> = <[u8; FIELD_LEN] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        let contract_name: Vec<u8> = <[u8; FIELD_LEN] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
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
}

impl ConsensusEncoding for ContractSpecification {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        copy_into_fixed_array_lossy::<_, 32>(&self.runtime).consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractSpecification {
    fn consensus_encode_exact_size(&self) -> usize {
        32
    }
}

impl ConsensusDecoding for ContractSpecification {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let runtime: Vec<u8> = <[u8; FIELD_LEN] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();

        Ok(Self { runtime })
    }
}

// #[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
// pub struct ContractSpecification {
// pub runtime: String,
// pub public_functions: Vec<PublicFunction>,
// pub initialization: Vec<FunctionCall>,
// }
//
// #[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
// pub struct PublicFunction {
// pub name: String, // TODO: limit it to 32 chars
// pub function: FunctionRef,
// pub argument_def: HashMap<String, ArgType>,
// }
//
// #[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
// pub struct FunctionCall {
// pub function: FunctionRef,
// pub arguments: HashMap<String, ArgType>,
// }
//
// #[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
// pub struct FunctionRef {
// pub template_func: String, // TODO: limit to 32 chars
// pub template_id: String,   // TODO: make it a hash
// }
//
// #[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
// pub enum ArgType {
// String,
// UInt64,
// }

#[cfg(test)]
mod test {
    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = ContractDefinitionFeatures {
            contract_id: str_to_padded_vec("contract_id"),
            contract_name: str_to_padded_vec("contract_name"),
            contract_issuer: PublicKey::default(),
            contract_spec: ContractSpecification {
                runtime: str_to_padded_vec("runtime"),
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
