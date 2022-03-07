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

use std::{
    cmp::Ordering,
    fmt,
    fmt::{Display, Formatter},
    io,
    io::{Read, Write},
};

use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, FixedHash, PublicKey};
use tari_crypto::ristretto::pedersen::PedersenCommitment;
use tari_utilities::ByteArray;

use super::OutputFeaturesVersion;
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes},
    transactions::{
        transaction_components::{
            AssetOutputFeatures,
            CommitteeDefinitionFeatures,
            MintNonFungibleFeatures,
            OutputFlags,
            SideChainCheckpointFeatures,
            TemplateParameter,
        },
        transaction_protocol::RewindData,
    },
};

/// Options for UTXO's
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct OutputFeatures {
    pub version: OutputFeaturesVersion,
    /// Flags are the feature flags that differentiate between outputs, eg Coinbase all of which has different rules
    pub flags: OutputFlags,
    /// the maturity of the specific UTXO. This is the min lock height at which an UTXO can be spent. Coinbase UTXO
    /// require a min maturity of the Coinbase_lock_height, this should be checked on receiving new blocks.
    pub maturity: u64,
    /// The recovery byte - not consensus critical - can help reduce the bandwidth with wallet recovery or in other
    /// instances when a wallet needs to request the complete UTXO set from a base node.
    pub recovery_byte: u8,
    pub metadata: Vec<u8>,
    pub unique_id: Option<Vec<u8>>,
    pub parent_public_key: Option<PublicKey>,
    pub asset: Option<AssetOutputFeatures>,
    pub mint_non_fungible: Option<MintNonFungibleFeatures>,
    pub sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
    pub committee_definition: Option<CommitteeDefinitionFeatures>,
}

impl OutputFeatures {
    pub const RECOVERY_BYTE_DEFAULT: u8 = 0;

    pub fn new(
        version: OutputFeaturesVersion,
        flags: OutputFlags,
        maturity: u64,
        recovery_byte: u8,
        metadata: Vec<u8>,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        asset: Option<AssetOutputFeatures>,
        mint_non_fungible: Option<MintNonFungibleFeatures>,
        sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
        committee_definition: Option<CommitteeDefinitionFeatures>,
    ) -> OutputFeatures {
        OutputFeatures {
            version,
            flags,
            maturity,
            recovery_byte,
            metadata,
            unique_id,
            parent_public_key,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            committee_definition,
        }
    }

    pub fn new_current_version(
        flags: OutputFlags,
        maturity: u64,
        recovery_byte: u8,
        metadata: Vec<u8>,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        asset: Option<AssetOutputFeatures>,
        mint_non_fungible: Option<MintNonFungibleFeatures>,
        sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
        committee_definition: Option<CommitteeDefinitionFeatures>,
    ) -> OutputFeatures {
        OutputFeatures::new(
            OutputFeaturesVersion::get_current_version(),
            flags,
            maturity,
            recovery_byte,
            metadata,
            unique_id,
            parent_public_key,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            committee_definition,
        )
    }

    pub fn create_coinbase(maturity_height: u64, recovery_byte: u8) -> OutputFeatures {
        OutputFeatures {
            flags: OutputFlags::COINBASE_OUTPUT,
            maturity: maturity_height,
            recovery_byte,
            ..Default::default()
        }
    }

    /// Helper function to create a unique recovery byte based on the commitment and a private recovery byte key,
    /// with the value '0' never obtainable as it is being reserved as a default value.
    pub fn create_unique_recovery_byte(commitment: &PedersenCommitment, rewind_data: Option<&RewindData>) -> u8 {
        let commitment_bytes = commitment.as_bytes();
        let recovery_key_bytes = if let Some(data) = rewind_data {
            data.recovery_byte_key.as_bytes()
        } else {
            &[]
        };
        const RECOVERY_BYTE_SIZE: usize = 1;
        let blake2_hasher = VarBlake2b::new(RECOVERY_BYTE_SIZE)
            .expect("Should be able to create blake2 hasher; will only panic if output size is 0 or greater than 64");
        let mut hash = [OutputFeatures::RECOVERY_BYTE_DEFAULT; RECOVERY_BYTE_SIZE];
        let mut salt = 0u64;
        loop {
            blake2_hasher
                .clone()
                .chain(commitment_bytes)
                .chain(recovery_key_bytes)
                .chain(b"hash my recovery byte")
                .chain(salt.to_le_bytes().as_slice())
                .finalize_variable(|res| hash.copy_from_slice(res));
            if hash[0] == OutputFeatures::RECOVERY_BYTE_DEFAULT {
                salt += 1;
            } else {
                break;
            }
        }
        hash[0]
    }

    /// Helper function to update the unique recovery byte if required and return updated output features
    pub fn update_recovery_byte_if_required(
        commitment: &PedersenCommitment,
        rewind_data: Option<&RewindData>,
        features: &OutputFeatures,
    ) -> OutputFeatures {
        let recovery_byte = OutputFeatures::create_unique_recovery_byte(commitment, rewind_data);
        let mut updated_features = features.clone();
        updated_features.set_recovery_byte(recovery_byte);
        updated_features
    }

    /// Provides the ability to update the recovery byte after the commitment has become known
    pub fn set_recovery_byte(&mut self, recovery_byte: u8) {
        self.recovery_byte = recovery_byte;
    }

    /// Provides the ability to add the recovery byte to an older version of serialized json output features. We need
    /// to add the non-optional 'recovery_byte' field to a persisted protocol that contains serialized json output
    /// features, typically when reading it from the database, if it was created with 'OutputFeaturesVersion::V0'
    pub fn add_recovery_byte_to_serialized_data_if_needed(protocol: String) -> String {
        // If the serialized data can be verified to contain output features, replace
        // ',\"metadata\":[' with ',\"recovery_byte\":0,\"metadata\":[' throughout
        let version_v0_identifier =
            format!("\"version\":\"{}\"", OutputFeaturesVersion::V0) + &String::from(",\"flags\":");
        let recovery_byte_identifier = String::from(",\"recovery_byte\":");
        let metadata_identifier = String::from(",\"metadata\":[");
        let unique_id_identifier = String::from("],\"unique_id\":");
        let mint_non_fungible_identifier = String::from(",\"mint_non_fungible\":");
        let sidechain_checkpoint_identifier = String::from(",\"sidechain_checkpoint\":");
        if protocol.contains(version_v0_identifier.as_str()) &&
            !protocol.contains(recovery_byte_identifier.as_str()) &&
            protocol.contains(unique_id_identifier.as_str()) &&
            protocol.contains(mint_non_fungible_identifier.as_str()) &&
            protocol.contains(sidechain_checkpoint_identifier.as_str())
        {
            let replace_string = recovery_byte_identifier +
                format!("{}", OutputFeatures::RECOVERY_BYTE_DEFAULT).as_str() +
                metadata_identifier.as_str();
            protocol.replace(metadata_identifier.as_str(), replace_string.as_str())
        } else {
            protocol
        }
    }

    pub fn for_asset_registration(
        metadata: Vec<u8>,
        public_key: PublicKey,
        template_ids_implemented: Vec<u32>,
        template_parameters: Vec<TemplateParameter>,
    ) -> OutputFeatures {
        let unique_id = Some(public_key.as_bytes().to_vec());
        Self {
            flags: OutputFlags::ASSET_REGISTRATION,
            maturity: 0,
            metadata,
            asset: Some(AssetOutputFeatures {
                public_key,
                template_ids_implemented,
                template_parameters,
            }),
            unique_id,
            ..Default::default()
        }
    }

    pub fn for_minting(
        asset_public_key: PublicKey,
        asset_owner_commitment: Commitment,
        unique_id: Vec<u8>,
        other_features: Option<OutputFeatures>,
    ) -> OutputFeatures {
        Self {
            flags: OutputFlags::MINT_NON_FUNGIBLE |
                other_features
                    .as_ref()
                    .map(|of| of.flags)
                    .unwrap_or_else(OutputFlags::empty),
            mint_non_fungible: Some(MintNonFungibleFeatures {
                asset_public_key: asset_public_key.clone(),
                asset_owner_commitment,
            }),
            parent_public_key: Some(asset_public_key),
            unique_id: Some(unique_id),
            ..other_features.unwrap_or_default()
        }
    }

    pub fn for_checkpoint(
        parent_public_key: PublicKey,
        unique_id: Vec<u8>,
        merkle_root: FixedHash,
        committee: Vec<PublicKey>,
        is_initial: bool,
    ) -> OutputFeatures {
        Self {
            flags: if is_initial {
                OutputFlags::SIDECHAIN_CHECKPOINT | OutputFlags::MINT_NON_FUNGIBLE
            } else {
                OutputFlags::SIDECHAIN_CHECKPOINT
            },
            sidechain_checkpoint: Some(SideChainCheckpointFeatures { merkle_root, committee }),
            parent_public_key: Some(parent_public_key),
            unique_id: Some(unique_id),
            ..Default::default()
        }
    }

    pub fn for_committee(
        parent_public_key: PublicKey,
        unique_id: Vec<u8>,
        committee: Vec<PublicKey>,
        effective_sidechain_height: u64,
        is_initial: bool,
    ) -> OutputFeatures {
        Self {
            flags: if is_initial {
                OutputFlags::COMMITTEE_DEFINITION | OutputFlags::MINT_NON_FUNGIBLE
            } else {
                OutputFlags::COMMITTEE_DEFINITION
            },
            committee_definition: Some(CommitteeDefinitionFeatures {
                committee,
                effective_sidechain_height,
            }),
            parent_public_key: Some(parent_public_key),
            unique_id: Some(unique_id),
            ..Default::default()
        }
    }

    pub fn unique_asset_id(&self) -> Option<&[u8]> {
        self.unique_id.as_deref()
    }

    pub fn is_non_fungible_mint(&self) -> bool {
        self.flags.contains(OutputFlags::MINT_NON_FUNGIBLE)
    }

    pub fn is_non_fungible_burn(&self) -> bool {
        self.flags.contains(OutputFlags::BURN_NON_FUNGIBLE)
    }

    pub fn is_coinbase(&self) -> bool {
        self.flags.contains(OutputFlags::COINBASE_OUTPUT)
    }

    fn consensus_encode_recovery_byte<W: Write>(recovery_byte: u8, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&[recovery_byte])?;
        Ok(1)
    }

    fn consensus_decode_recovery_byte<R: Read>(reader: &mut R) -> Result<u8, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let recovery_byte = buf[0] as u8;
        Ok(recovery_byte)
    }
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = self.version.consensus_encode(writer)?;
        written += self.maturity.consensus_encode(writer)?;
        written += self.flags.consensus_encode(writer)?;
        match self.version {
            OutputFeaturesVersion::V0 => (),
            OutputFeaturesVersion::V1 => {
                written += OutputFeatures::consensus_encode_recovery_byte(self.recovery_byte, writer)?;
            },
        }
        written += self.parent_public_key.consensus_encode(writer)?;
        written += self.unique_id.consensus_encode(writer)?;
        written += self.asset.consensus_encode(writer)?;
        written += self.mint_non_fungible.consensus_encode(writer)?;
        written += self.sidechain_checkpoint.consensus_encode(writer)?;
        written += self.metadata.consensus_encode(writer)?;
        match self.version {
            OutputFeaturesVersion::V0 => (),
            OutputFeaturesVersion::V1 => {
                written += self.committee_definition.consensus_encode(writer)?;
            },
        }
        Ok(written)
    }
}

impl ConsensusEncodingSized for OutputFeatures {}

impl ConsensusDecoding for OutputFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        // Changing the order of these operations is consensus breaking
        // Decode safety: consensus_decode will stop reading the varint after 10 bytes
        let version = OutputFeaturesVersion::consensus_decode(reader)?;
        let maturity = u64::consensus_decode(reader)?;
        let flags = OutputFlags::consensus_decode(reader)?;
        let recovery_byte = match version {
            OutputFeaturesVersion::V0 => OutputFeatures::RECOVERY_BYTE_DEFAULT,
            OutputFeaturesVersion::V1 => OutputFeatures::consensus_decode_recovery_byte(reader)?,
        };
        let parent_public_key = <Option<PublicKey> as ConsensusDecoding>::consensus_decode(reader)?;
        const MAX_UNIQUE_ID_SIZE: usize = 256;
        let unique_id = <Option<MaxSizeBytes<MAX_UNIQUE_ID_SIZE>> as ConsensusDecoding>::consensus_decode(reader)?;
        let asset = <Option<AssetOutputFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        let mint_non_fungible = <Option<MintNonFungibleFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        let sidechain_checkpoint =
            <Option<SideChainCheckpointFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        const MAX_METADATA_SIZE: usize = 1024;
        let metadata = <MaxSizeBytes<MAX_METADATA_SIZE> as ConsensusDecoding>::consensus_decode(reader)?;
        let committee_definition = match version {
            OutputFeaturesVersion::V0 => None,
            OutputFeaturesVersion::V1 => {
                <Option<CommitteeDefinitionFeatures> as ConsensusDecoding>::consensus_decode(reader)?
            },
        };
        Ok(Self {
            version,
            flags,
            maturity,
            recovery_byte,
            parent_public_key,
            unique_id: unique_id.map(Into::into),
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            metadata: metadata.into(),
            committee_definition,
        })
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures::new_current_version(
            OutputFlags::empty(),
            0,
            OutputFeatures::RECOVERY_BYTE_DEFAULT,
            vec![],
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }
}

impl PartialOrd for OutputFeatures {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OutputFeatures {
    fn cmp(&self, other: &Self) -> Ordering {
        self.maturity.cmp(&other.maturity)
    }
}

impl Display for OutputFeatures {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OutputFeatures: Flags = {:?}, Maturity = {}, recovery byte = {:#08b}",
            self.flags, self.maturity, self.recovery_byte
        )
    }
}

#[cfg(test)]
mod test {
    use std::{io::ErrorKind, iter};

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    fn make_fully_populated_output_features(version: OutputFeaturesVersion) -> OutputFeatures {
        OutputFeatures {
            version,
            flags: OutputFlags::all(),
            maturity: u64::MAX,
            recovery_byte: match version {
                OutputFeaturesVersion::V0 => OutputFeatures::RECOVERY_BYTE_DEFAULT,
                OutputFeaturesVersion::V1 => u8::MAX,
            },
            metadata: vec![1; 1024],
            unique_id: Some(vec![0u8; 256]),
            parent_public_key: Some(PublicKey::default()),
            asset: Some(AssetOutputFeatures {
                public_key: Default::default(),
                template_ids_implemented: vec![1u32; 50],
                template_parameters: iter::repeat_with(|| TemplateParameter {
                    template_id: 0,
                    template_data_version: 0,
                    template_data: vec![],
                })
                .take(50)
                .collect(),
            }),
            mint_non_fungible: Some(MintNonFungibleFeatures {
                asset_public_key: Default::default(),
                asset_owner_commitment: Default::default(),
            }),
            sidechain_checkpoint: Some(SideChainCheckpointFeatures {
                merkle_root: [1u8; 32],
                committee: iter::repeat_with(PublicKey::default).take(50).collect(),
            }),
            committee_definition: match version {
                OutputFeaturesVersion::V0 => None,
                OutputFeaturesVersion::V1 => Some(CommitteeDefinitionFeatures {
                    committee: iter::repeat_with(PublicKey::default).take(50).collect(),
                    effective_sidechain_height: u64::MAX,
                }),
            },
        }
    }

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = make_fully_populated_output_features(OutputFeaturesVersion::V0);
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_encodes_and_decodes_correctly_in_none_case() {
        let mut subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        subject.unique_id = None;
        subject.asset = None;
        subject.mint_non_fungible = None;
        subject.sidechain_checkpoint = None;
        subject.committee_definition = None;
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_fails_for_large_metadata() {
        let mut subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        subject.metadata = vec![1u8; 1025];
        let err = check_consensus_encoding_correctness(subject).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn it_fails_for_large_unique_id() {
        let mut subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        subject.unique_id = Some(vec![0u8; 257]);

        let err = check_consensus_encoding_correctness(subject).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn test_output_features_version_update() {
        // Transaction protocol
        let transaction_protocol_v0_no_recovery_byte = String::from(
            r#"{"offset":"4706e142d3d4e471895daadfac91dd5788d3b8eb146e572a4df4816cf3965e04","body":{"sorted":true,"inputs":[{"version":"V0","spent_output":{"OutputData":{"version":"V0","features":{"version":"V0","flags":{"bits":1},"maturity":1150,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"4aaf5f47d880fdb998069ef88ae72e3e74efee9963b24636a0538bd2a50f8e2e","script":"73","sender_offset_public_key":"70fd74f86a0507e05422f664a67445ec6cfb66852078886056c329d1d15d0953","covenant":""}},"input_data":{"items":[{"PublicKey":"ec68cfb5a76bef2e278bdb99cf35b5bfc4a61711d673222629dfaee955364235"}]},"script_signature":{"public_nonce":"009df6c15bad9b1edcf1ea7209cd84a69e2ebba08b65ea0e40dfa35c9717dc63","u":"1fd632a7123acb642cb485271b1f12ce5948a2e3c505547cd445afd0ebf6750c","v":"96d78ddd71b723939c86d3df62d42d57d188a21b2fee773270eb6fac5db08e01"}}],"outputs":[{"version":"V0","features":{"version":"V0","flags":{"bits":0},"maturity":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"1e272097305de163cf4c8b411b5a0d6548f7d0153f711dea67afe8c8416fcf76","proof":"b4fbd20f1c0b6c6258bb1326def41774dde59201579c0fcb58bc5d27cd936d20ec4a6a04be80266ecc7cc9bdd918a650dcd899e681edf50790db3e1505b38c3640aeca757b4811d6fc51a7f67e2a82439c372e71f68907ac837831a9403f8c4b308b883391ba607fb5d2b270d1d06e3ff4015970068fc1f31c8c94a56d44ce39a5b25c713b1baaffe4839e718c554a8ae7ba4aea2a1773ce8c04deab3853410faedc82b43d7bd0f19b381b43987b1a631d64f2cad3ae0c86bbc74ab6b03f5603d3f2201e8df837e76c481102057709c2c74dcf8ef33dc1c6dd93a088d7b3bf053637b681510e9371c6b8c14253b9bebdc813dca4a36477c5d133e6448f20934d96315a952e011cafdb102f9e72922e929d2985c6168bcdf093d75e0887cc504fea89a1783e5f2cdbdedaa5b9020c601b7604d254242b153fc46eafc93ffdb0654616ac904679e38ceaaf93a1e9a062d129dfbaca342bbc89827a46fb1bd2a61da614ef5c8c5dc02b1d081626576e4ccac26d3ba058713f6e9e001f5dec31582e4c6a93b01c37c295a61cca2cd2046dd692a58fdf33ac7a415e4565f6c0c16d34fc92e6337c9a3fdb52155238983a4fe552ecbcebc671f6ccab2bed3b8639835c98b34943ba5b814f0966d1d8c5bf8866da33d9474e2f74165676a9894cc70d3baae0e913eed7a431ccf26523c8636ef3bc74dfce9c10e6fa9cf3d34c9f8b0c428823190236f96932bbcba22a16af5c7caf2b47ed83b2d8d8efa565a121c91472da51fca5aebbbab07dd96a90cd0101ab94939404e45626a229122932488529757e2388c92e4a308e2a4b2f69e9e6c96822d725fbf10f8a013f8b12a4ba7b9a15c17cc2dd06d87280df828a68aa7902f33bd6553b95b23f8ecadacd971283e5095e32757d4f802ad939ccfacfa6921c704080dcd457872b8b55098b668b1c5208","script":"73","sender_offset_public_key":"305602d229ee72c5225c1ce2417b8617ba216bff3e3ca421254157a5ea97bf5c","metadata_signature":{"public_nonce":"f6de90facb5601e17a1ebf9733d50d3d878ca9597e3e239164c49985a0215c37","u":"d45b80b95d0f1f0d30659ec3d851a59c938f44ba5ecbff42a53de61fa0629a0a","v":"98b858f8e8c560733b663dd485d18c235e1b4d2765aa8c0d68cd4c924505da0e"},"covenant":""},{"version":"V0","features":{"version":"V0","flags":{"bits":0},"maturity":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"26e436b5793abdcf7f28e3e7f9021ac5077f1585cea504e879cf2ff6ce0b5753","proof":"a2b3e54d1085bc0e5cf344f9dce3e70cc0ab2d6751aa08d3dbfe262a7b16f85a383cc7faa142186f23fcbd1513bf80712a8c67877fbb4eae3bd379b0734c0111463109a1b7c9329ee1befc81c719d3d3f87d8a8cf4b1a0accfc1daed01dc43446c14fd3dc66462ad7e0e65e4df1b925866f223a9a5e9943bfa6eb4ff6411ba2f87abe025dd407aee81ca6d594e7cd0ba60e500c26dbfde8beed434a553f32d0fa257123ade664ab37181987420396b300e2881e97944ef7bd346746efa43520ae896fe1de7972e43781f5b29d9845d864390f4821fb329525fbb4e1f31709b04f65dcd9636ef30727ff0ed83ebea62379b874728806bd1e1bde564501615650d0e0aa1b1bd2b535b8bf3f1b9be2e22c446b8819cc7998e69d27bb478a40a212dd88923c4200ad19b7f908f78bdec548f4a61a9bd7b43e8d801aed609e08e4077889f84cc54232b09a117e6a964a5b047409ddc03b992c8daf4f84c1ffc68e413a6c1f86939681b43e3df3a0eaf758b3326054d315a1eea95bc040fe46266f57c6469a9f36bc38b9e184cceb52ae0dc5130cf688c08e035a34dcfb5ae9f5fd378f4fbb5a482ebb251e0d0f9536ea3544c0d92a82c38be46f1786ee5fa4865ee04faa9d56c3746642d08e4fef0c586761d7b26760e49c929c033c8363644595b22842d0af9866f18f4b03db04830b52bba0f167a775fa4fa80ee26a6f613a077535ead4235d42007e588deb1d59b89a617817dd2e0065b8f8ef19c0748ffb9f173aef3257f833f897224e7bf52f683ca65e647a03884cd0cc61231e1416cf6160f7460b1d5c8fc98b488dc1a9d095a9248cc4936c78da735d404e50e929329bd2146ad92b56f3818a50b3938418c3be7223c296ec4376e2bb84aaf56478a230101fd12ebdc4cc510915ebc58483afa83dfc23f4194b636eb807cc42f3f1f9c090f","script":"73","sender_offset_public_key":"62f4a8e061dd49c912e59a26979f8686f4ae4496b471b70bda840ab49800ca25","metadata_signature":{"public_nonce":"9ed346aa4c2f5e23dc868b3ecae15e41bf658debeb3eb9db1644c8c962a9b45e","u":"c7fddc9dbdcef545ce83d012ad7d1f4235b1f29ef0354e43bdd2545adf763b09","v":"01830a2052e8c497edad8c289ed456c49cfabd6cf048675dc15c10f72b49800c"},"covenant":""}],"kernels":[{"version":"V0","features":{"bits":0},"fee":630,"lock_height":0,"excess":"34b998e0b07f5b0e943c8f0da1efa5bbc9723eb322dbc656667347576b951a71","excess_sig":{"public_nonce":"545b9e5d248565520a886970ed3aead9f214a9837018284fb8a2abc2c72f132a","signature":"3e96aef7c8bcb1241aa2675a661d610c1fc4012bb876c8822550983d1fc2190f"}}]},"script_offset":"76ec8d7cb5c345159ddb965064d75ca69b2ba527069753c9e127ed5ba20bce0d"}"#,
        );
        let transaction_protocol_v0_updated = String::from(
            r#"{"offset":"4706e142d3d4e471895daadfac91dd5788d3b8eb146e572a4df4816cf3965e04","body":{"sorted":true,"inputs":[{"version":"V0","spent_output":{"OutputData":{"version":"V0","features":{"version":"V0","flags":{"bits":1},"maturity":1150,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"4aaf5f47d880fdb998069ef88ae72e3e74efee9963b24636a0538bd2a50f8e2e","script":"73","sender_offset_public_key":"70fd74f86a0507e05422f664a67445ec6cfb66852078886056c329d1d15d0953","covenant":""}},"input_data":{"items":[{"PublicKey":"ec68cfb5a76bef2e278bdb99cf35b5bfc4a61711d673222629dfaee955364235"}]},"script_signature":{"public_nonce":"009df6c15bad9b1edcf1ea7209cd84a69e2ebba08b65ea0e40dfa35c9717dc63","u":"1fd632a7123acb642cb485271b1f12ce5948a2e3c505547cd445afd0ebf6750c","v":"96d78ddd71b723939c86d3df62d42d57d188a21b2fee773270eb6fac5db08e01"}}],"outputs":[{"version":"V0","features":{"version":"V0","flags":{"bits":0},"maturity":0,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"1e272097305de163cf4c8b411b5a0d6548f7d0153f711dea67afe8c8416fcf76","proof":"b4fbd20f1c0b6c6258bb1326def41774dde59201579c0fcb58bc5d27cd936d20ec4a6a04be80266ecc7cc9bdd918a650dcd899e681edf50790db3e1505b38c3640aeca757b4811d6fc51a7f67e2a82439c372e71f68907ac837831a9403f8c4b308b883391ba607fb5d2b270d1d06e3ff4015970068fc1f31c8c94a56d44ce39a5b25c713b1baaffe4839e718c554a8ae7ba4aea2a1773ce8c04deab3853410faedc82b43d7bd0f19b381b43987b1a631d64f2cad3ae0c86bbc74ab6b03f5603d3f2201e8df837e76c481102057709c2c74dcf8ef33dc1c6dd93a088d7b3bf053637b681510e9371c6b8c14253b9bebdc813dca4a36477c5d133e6448f20934d96315a952e011cafdb102f9e72922e929d2985c6168bcdf093d75e0887cc504fea89a1783e5f2cdbdedaa5b9020c601b7604d254242b153fc46eafc93ffdb0654616ac904679e38ceaaf93a1e9a062d129dfbaca342bbc89827a46fb1bd2a61da614ef5c8c5dc02b1d081626576e4ccac26d3ba058713f6e9e001f5dec31582e4c6a93b01c37c295a61cca2cd2046dd692a58fdf33ac7a415e4565f6c0c16d34fc92e6337c9a3fdb52155238983a4fe552ecbcebc671f6ccab2bed3b8639835c98b34943ba5b814f0966d1d8c5bf8866da33d9474e2f74165676a9894cc70d3baae0e913eed7a431ccf26523c8636ef3bc74dfce9c10e6fa9cf3d34c9f8b0c428823190236f96932bbcba22a16af5c7caf2b47ed83b2d8d8efa565a121c91472da51fca5aebbbab07dd96a90cd0101ab94939404e45626a229122932488529757e2388c92e4a308e2a4b2f69e9e6c96822d725fbf10f8a013f8b12a4ba7b9a15c17cc2dd06d87280df828a68aa7902f33bd6553b95b23f8ecadacd971283e5095e32757d4f802ad939ccfacfa6921c704080dcd457872b8b55098b668b1c5208","script":"73","sender_offset_public_key":"305602d229ee72c5225c1ce2417b8617ba216bff3e3ca421254157a5ea97bf5c","metadata_signature":{"public_nonce":"f6de90facb5601e17a1ebf9733d50d3d878ca9597e3e239164c49985a0215c37","u":"d45b80b95d0f1f0d30659ec3d851a59c938f44ba5ecbff42a53de61fa0629a0a","v":"98b858f8e8c560733b663dd485d18c235e1b4d2765aa8c0d68cd4c924505da0e"},"covenant":""},{"version":"V0","features":{"version":"V0","flags":{"bits":0},"maturity":0,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"26e436b5793abdcf7f28e3e7f9021ac5077f1585cea504e879cf2ff6ce0b5753","proof":"a2b3e54d1085bc0e5cf344f9dce3e70cc0ab2d6751aa08d3dbfe262a7b16f85a383cc7faa142186f23fcbd1513bf80712a8c67877fbb4eae3bd379b0734c0111463109a1b7c9329ee1befc81c719d3d3f87d8a8cf4b1a0accfc1daed01dc43446c14fd3dc66462ad7e0e65e4df1b925866f223a9a5e9943bfa6eb4ff6411ba2f87abe025dd407aee81ca6d594e7cd0ba60e500c26dbfde8beed434a553f32d0fa257123ade664ab37181987420396b300e2881e97944ef7bd346746efa43520ae896fe1de7972e43781f5b29d9845d864390f4821fb329525fbb4e1f31709b04f65dcd9636ef30727ff0ed83ebea62379b874728806bd1e1bde564501615650d0e0aa1b1bd2b535b8bf3f1b9be2e22c446b8819cc7998e69d27bb478a40a212dd88923c4200ad19b7f908f78bdec548f4a61a9bd7b43e8d801aed609e08e4077889f84cc54232b09a117e6a964a5b047409ddc03b992c8daf4f84c1ffc68e413a6c1f86939681b43e3df3a0eaf758b3326054d315a1eea95bc040fe46266f57c6469a9f36bc38b9e184cceb52ae0dc5130cf688c08e035a34dcfb5ae9f5fd378f4fbb5a482ebb251e0d0f9536ea3544c0d92a82c38be46f1786ee5fa4865ee04faa9d56c3746642d08e4fef0c586761d7b26760e49c929c033c8363644595b22842d0af9866f18f4b03db04830b52bba0f167a775fa4fa80ee26a6f613a077535ead4235d42007e588deb1d59b89a617817dd2e0065b8f8ef19c0748ffb9f173aef3257f833f897224e7bf52f683ca65e647a03884cd0cc61231e1416cf6160f7460b1d5c8fc98b488dc1a9d095a9248cc4936c78da735d404e50e929329bd2146ad92b56f3818a50b3938418c3be7223c296ec4376e2bb84aaf56478a230101fd12ebdc4cc510915ebc58483afa83dfc23f4194b636eb807cc42f3f1f9c090f","script":"73","sender_offset_public_key":"62f4a8e061dd49c912e59a26979f8686f4ae4496b471b70bda840ab49800ca25","metadata_signature":{"public_nonce":"9ed346aa4c2f5e23dc868b3ecae15e41bf658debeb3eb9db1644c8c962a9b45e","u":"c7fddc9dbdcef545ce83d012ad7d1f4235b1f29ef0354e43bdd2545adf763b09","v":"01830a2052e8c497edad8c289ed456c49cfabd6cf048675dc15c10f72b49800c"},"covenant":""}],"kernels":[{"version":"V0","features":{"bits":0},"fee":630,"lock_height":0,"excess":"34b998e0b07f5b0e943c8f0da1efa5bbc9723eb322dbc656667347576b951a71","excess_sig":{"public_nonce":"545b9e5d248565520a886970ed3aead9f214a9837018284fb8a2abc2c72f132a","signature":"3e96aef7c8bcb1241aa2675a661d610c1fc4012bb876c8822550983d1fc2190f"}}]},"script_offset":"76ec8d7cb5c345159ddb965064d75ca69b2ba527069753c9e127ed5ba20bce0d"}"#,
        );
        assert_eq!(
            transaction_protocol_v0_updated,
            OutputFeatures::add_recovery_byte_to_serialized_data_if_needed(transaction_protocol_v0_no_recovery_byte)
        );
        assert_eq!(
            transaction_protocol_v0_updated,
            OutputFeatures::add_recovery_byte_to_serialized_data_if_needed(transaction_protocol_v0_updated.clone())
        );
        let transaction_protocol_v1 = String::from(
            r#"{"offset":"4706e142d3d4e471895daadfac91dd5788d3b8eb146e572a4df4816cf3965e04","body":{"sorted":true,"inputs":[{"version":"V0","spent_output":{"OutputData":{"version":"V0","features":{"version":"V1","flags":{"bits":1},"maturity":1150,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"4aaf5f47d880fdb998069ef88ae72e3e74efee9963b24636a0538bd2a50f8e2e","script":"73","sender_offset_public_key":"70fd74f86a0507e05422f664a67445ec6cfb66852078886056c329d1d15d0953","covenant":""}},"input_data":{"items":[{"PublicKey":"ec68cfb5a76bef2e278bdb99cf35b5bfc4a61711d673222629dfaee955364235"}]},"script_signature":{"public_nonce":"009df6c15bad9b1edcf1ea7209cd84a69e2ebba08b65ea0e40dfa35c9717dc63","u":"1fd632a7123acb642cb485271b1f12ce5948a2e3c505547cd445afd0ebf6750c","v":"96d78ddd71b723939c86d3df62d42d57d188a21b2fee773270eb6fac5db08e01"}}],"outputs":[{"version":"V0","features":{"version":"V1","flags":{"bits":0},"maturity":0,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"1e272097305de163cf4c8b411b5a0d6548f7d0153f711dea67afe8c8416fcf76","proof":"b4fbd20f1c0b6c6258bb1326def41774dde59201579c0fcb58bc5d27cd936d20ec4a6a04be80266ecc7cc9bdd918a650dcd899e681edf50790db3e1505b38c3640aeca757b4811d6fc51a7f67e2a82439c372e71f68907ac837831a9403f8c4b308b883391ba607fb5d2b270d1d06e3ff4015970068fc1f31c8c94a56d44ce39a5b25c713b1baaffe4839e718c554a8ae7ba4aea2a1773ce8c04deab3853410faedc82b43d7bd0f19b381b43987b1a631d64f2cad3ae0c86bbc74ab6b03f5603d3f2201e8df837e76c481102057709c2c74dcf8ef33dc1c6dd93a088d7b3bf053637b681510e9371c6b8c14253b9bebdc813dca4a36477c5d133e6448f20934d96315a952e011cafdb102f9e72922e929d2985c6168bcdf093d75e0887cc504fea89a1783e5f2cdbdedaa5b9020c601b7604d254242b153fc46eafc93ffdb0654616ac904679e38ceaaf93a1e9a062d129dfbaca342bbc89827a46fb1bd2a61da614ef5c8c5dc02b1d081626576e4ccac26d3ba058713f6e9e001f5dec31582e4c6a93b01c37c295a61cca2cd2046dd692a58fdf33ac7a415e4565f6c0c16d34fc92e6337c9a3fdb52155238983a4fe552ecbcebc671f6ccab2bed3b8639835c98b34943ba5b814f0966d1d8c5bf8866da33d9474e2f74165676a9894cc70d3baae0e913eed7a431ccf26523c8636ef3bc74dfce9c10e6fa9cf3d34c9f8b0c428823190236f96932bbcba22a16af5c7caf2b47ed83b2d8d8efa565a121c91472da51fca5aebbbab07dd96a90cd0101ab94939404e45626a229122932488529757e2388c92e4a308e2a4b2f69e9e6c96822d725fbf10f8a013f8b12a4ba7b9a15c17cc2dd06d87280df828a68aa7902f33bd6553b95b23f8ecadacd971283e5095e32757d4f802ad939ccfacfa6921c704080dcd457872b8b55098b668b1c5208","script":"73","sender_offset_public_key":"305602d229ee72c5225c1ce2417b8617ba216bff3e3ca421254157a5ea97bf5c","metadata_signature":{"public_nonce":"f6de90facb5601e17a1ebf9733d50d3d878ca9597e3e239164c49985a0215c37","u":"d45b80b95d0f1f0d30659ec3d851a59c938f44ba5ecbff42a53de61fa0629a0a","v":"98b858f8e8c560733b663dd485d18c235e1b4d2765aa8c0d68cd4c924505da0e"},"covenant":""},{"version":"V0","features":{"version":"V1","flags":{"bits":0},"maturity":0,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null},"commitment":"26e436b5793abdcf7f28e3e7f9021ac5077f1585cea504e879cf2ff6ce0b5753","proof":"a2b3e54d1085bc0e5cf344f9dce3e70cc0ab2d6751aa08d3dbfe262a7b16f85a383cc7faa142186f23fcbd1513bf80712a8c67877fbb4eae3bd379b0734c0111463109a1b7c9329ee1befc81c719d3d3f87d8a8cf4b1a0accfc1daed01dc43446c14fd3dc66462ad7e0e65e4df1b925866f223a9a5e9943bfa6eb4ff6411ba2f87abe025dd407aee81ca6d594e7cd0ba60e500c26dbfde8beed434a553f32d0fa257123ade664ab37181987420396b300e2881e97944ef7bd346746efa43520ae896fe1de7972e43781f5b29d9845d864390f4821fb329525fbb4e1f31709b04f65dcd9636ef30727ff0ed83ebea62379b874728806bd1e1bde564501615650d0e0aa1b1bd2b535b8bf3f1b9be2e22c446b8819cc7998e69d27bb478a40a212dd88923c4200ad19b7f908f78bdec548f4a61a9bd7b43e8d801aed609e08e4077889f84cc54232b09a117e6a964a5b047409ddc03b992c8daf4f84c1ffc68e413a6c1f86939681b43e3df3a0eaf758b3326054d315a1eea95bc040fe46266f57c6469a9f36bc38b9e184cceb52ae0dc5130cf688c08e035a34dcfb5ae9f5fd378f4fbb5a482ebb251e0d0f9536ea3544c0d92a82c38be46f1786ee5fa4865ee04faa9d56c3746642d08e4fef0c586761d7b26760e49c929c033c8363644595b22842d0af9866f18f4b03db04830b52bba0f167a775fa4fa80ee26a6f613a077535ead4235d42007e588deb1d59b89a617817dd2e0065b8f8ef19c0748ffb9f173aef3257f833f897224e7bf52f683ca65e647a03884cd0cc61231e1416cf6160f7460b1d5c8fc98b488dc1a9d095a9248cc4936c78da735d404e50e929329bd2146ad92b56f3818a50b3938418c3be7223c296ec4376e2bb84aaf56478a230101fd12ebdc4cc510915ebc58483afa83dfc23f4194b636eb807cc42f3f1f9c090f","script":"73","sender_offset_public_key":"62f4a8e061dd49c912e59a26979f8686f4ae4496b471b70bda840ab49800ca25","metadata_signature":{"public_nonce":"9ed346aa4c2f5e23dc868b3ecae15e41bf658debeb3eb9db1644c8c962a9b45e","u":"c7fddc9dbdcef545ce83d012ad7d1f4235b1f29ef0354e43bdd2545adf763b09","v":"01830a2052e8c497edad8c289ed456c49cfabd6cf048675dc15c10f72b49800c"},"covenant":""}],"kernels":[{"version":"V0","features":{"bits":0},"fee":630,"lock_height":0,"excess":"34b998e0b07f5b0e943c8f0da1efa5bbc9723eb322dbc656667347576b951a71","excess_sig":{"public_nonce":"545b9e5d248565520a886970ed3aead9f214a9837018284fb8a2abc2c72f132a","signature":"3e96aef7c8bcb1241aa2675a661d610c1fc4012bb876c8822550983d1fc2190f"}}]},"script_offset":"76ec8d7cb5c345159ddb965064d75ca69b2ba527069753c9e127ed5ba20bce0d"}"#,
        );
        assert_eq!(
            transaction_protocol_v1,
            OutputFeatures::add_recovery_byte_to_serialized_data_if_needed(transaction_protocol_v1.clone())
        );

        // Output
        let output_v0_no_recovery_byte = String::from(
            r#"{"version":"V0","flags":{"bits":1},"maturity":1150,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null}"#,
        );
        let output_v0_no_recovery_byte_updated = String::from(
            r#"{"version":"V0","flags":{"bits":1},"maturity":1150,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null}"#,
        );
        assert_eq!(
            output_v0_no_recovery_byte_updated,
            OutputFeatures::add_recovery_byte_to_serialized_data_if_needed(output_v0_no_recovery_byte)
        );
        assert_eq!(
            output_v0_no_recovery_byte_updated,
            OutputFeatures::add_recovery_byte_to_serialized_data_if_needed(output_v0_no_recovery_byte_updated.clone())
        );
        let output_v1 = String::from(
            r#"{"version":"V1","flags":{"bits":1},"maturity":1150,"recovery_byte":0,"metadata":[],"unique_id":null,"parent_public_key":null,"asset":null,"mint_non_fungible":null,"sidechain_checkpoint":null}"#,
        );
        assert_eq!(
            output_v1,
            OutputFeatures::add_recovery_byte_to_serialized_data_if_needed(output_v1.clone())
        );
    }
}
