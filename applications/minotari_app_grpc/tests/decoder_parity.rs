// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Decoder parity for blocks and transactions.
//!
//! A node decodes the same data through up to five decoders: serde_json (HTTP JSON-RPC), bincode, borsh (the chain
//! database and `submit_block_blob`), the P2P protobuf conversions in `tari_core::proto` and the gRPC protobuf
//! conversions in this crate. If one of them accepts a value another rejects, a node can accept data every peer
//! refuses, so its block templates or its chain fork off.
//!
//! Every sample below is a change to one field of a valid block or transaction, applied to each encoding.
//! [`assert_parity`] checks that every decoder makes the same decision on it and, when they accept it, decodes the
//! same value. Adding a check to one decoder family and not the other fails this test.

#![cfg(feature = "base_node")]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::fmt::Debug;

use borsh::{BorshDeserialize, BorshSerialize};
use minotari_app_grpc::tari_rpc as grpc;
use rand::RngExt;
use serde_json::Value;
use tari_common::configuration::Network;
use tari_common_types::{
    epoch::VnEpoch,
    types::{
        ComAndPubSignature,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        FixedHash,
        PrivateKey,
    },
};
use tari_core::{blocks::genesis_block::get_genesis_block, proto};
use tari_crypto::keys::SecretKey;
use tari_max_size::{ValidatedDecode, validated_decode::DecodesViaValidatedDecode};
use tari_node_components::blocks::{Block, BlockHeader};
use tari_script::{ExecutionStack, MAX_SCRIPT_BYTES, StackItem, TariScript, script};
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    covenant,
    key_manager::TariKeyId,
    tari_proof_of_work::{Difficulty, PowAlgorithm},
    transaction_components::{
        BuildInfo,
        CodeTemplateRegistration,
        ConfidentialOutputData,
        EncryptedData,
        KernelFeatures,
        MemoField,
        OutputFeaturesVersion,
        OutputType,
        RangeProofType,
        SideChainFeature,
        SideChainFeatureData,
        SideChainId,
        SpentOutput,
        TemplateType,
        Transaction,
        TransactionInput,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutputVersion,
        ValidatorNodeExit,
        ValidatorNodeRegistration,
        ValidatorNodeSignature,
        covenants::Covenant,
        encrypted_data::{MAX_ENCRYPTED_DATA_SIZE, STATIC_ENCRYPTED_DATA_SIZE_TOTAL},
    },
};
use tari_utilities::{ByteArray, hex::to_hex};

/// Asserts that every decoder made the expected decision on `sample`: either all rejected it, or all accepted it and
/// decoded it to the same value. `results` holds the name of each decoder and its result (`None` if it rejected the
/// sample).
fn assert_parity<T: PartialEq + Debug>(sample: &str, accept: bool, results: &[(&str, Option<T>)]) {
    let verdict = |result: &Option<T>| if result.is_some() { "accepted" } else { "rejected" };
    let (first_name, first) = results.first().expect("at least one decoder");
    assert_eq!(
        first.is_some(),
        accept,
        "{sample}: expected every decoder to {} it, but {first_name} {} it",
        if accept { "accept" } else { "reject" },
        verdict(first)
    );
    for (name, result) in results {
        assert_eq!(
            result.is_some(),
            first.is_some(),
            "{sample}: {first_name} {} it, but {name} {} it",
            verdict(first),
            verdict(result)
        );
        assert!(
            result == first,
            "{sample}: {first_name} and {name} decoded different values"
        );
    }
}

/// A change to one field of the base block or transaction. Body changes apply to the first output, input or kernel.
#[derive(Debug, Clone)]
enum Mutation {
    /// No change: the base value itself
    Nothing,
    // The first output
    OutputEncryptedData(Vec<u8>),
    OutputScript(Vec<u8>),
    OutputCovenant(Vec<u8>),
    OutputFeaturesVersion(u32),
    OutputType(u32),
    OutputRangeProofType(u32),
    OutputCoinbaseExtra(Vec<u8>),
    OutputRangeProof(Vec<u8>),
    OutputCommitment(Vec<u8>),
    OutputSenderOffsetPublicKey(Vec<u8>),
    OutputMetadataSignatureUa(Vec<u8>),
    OutputVersion(u32),
    /// Installs a (valid) side-chain feature on the first output, then changes one of its fields
    OutputSideChain(Box<SideChainFeature>, SideChainChange),
    // The first (compact) input
    InputData(Vec<u8>),
    InputScriptSignatureEphemeralPubkey(Vec<u8>),
    // The second (full, non-compact) input. The version of a compact input is not sampled: neither protobuf family
    // carries it (both decode a compact input with the current version), while serde and borsh keep it. That known
    // divergence is pinned by `the_protobuf_families_decode_a_compact_v1_input_as_v0`.
    InputVersion(u32),
    FullInputScript(Vec<u8>),
    FullInputEncryptedData(Vec<u8>),
    FullInputCommitment(Vec<u8>),
    FullInputSenderOffsetPublicKey(Vec<u8>),
    FullInputMetadataSignatureUa(Vec<u8>),
    FullInputOutputType(u32),
    FullInputRangeproofHash(Vec<u8>),
    // The first kernel
    KernelFeatures(u32),
    KernelExcess(Vec<u8>),
    KernelExcessSignature(Vec<u8>),
    KernelVersion(u32),
    KernelBurnCommitment(Vec<u8>),
    // Block only
    HeaderBlockOutputMr(Vec<u8>),
    HeaderHash(HeaderHash, Vec<u8>),
    HeaderVersion(u32),
    HeaderTotalKernelOffset(Vec<u8>),
    HeaderPowAlgo(u64),
    HeaderPowData(Vec<u8>),
    // Transaction only
    TransactionOffset(Vec<u8>),
}

/// The 32 byte hash fields of a block header (besides `block_output_mr`)
#[derive(Debug, Clone, Copy)]
enum HeaderHash {
    PrevHash,
    OutputMr,
    KernelMr,
    InputMr,
    ValidatorNodeMr,
}

/// A template type, as a template registration sample
#[derive(Debug, Clone, Copy)]
enum TemplateTypeSample {
    Wasm(u32),
    Flow,
    Manifest,
}

/// A change to one field of a side-chain feature
#[derive(Debug, Clone)]
enum SideChainChange {
    Nothing,
    /// The main public key of the feature data (validator key, template author key or claim key)
    DataPublicKey(Vec<u8>),
    RegistrationClaimPublicKey(Vec<u8>),
    TemplateName(String),
    TemplateVersion(u32),
    TemplateBinarySha(Vec<u8>),
    TemplateBinaryUrl(String),
    TemplateCommitHash(Vec<u8>),
    TemplateType(TemplateTypeSample),
    SidechainIdPublicKey(Vec<u8>),
    SidechainIdSignature(Vec<u8>),
}

fn random_public_key() -> CompressedPublicKey {
    CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()))
}

fn random_public_key_bytes() -> Vec<u8> {
    random_public_key().as_bytes().to_vec()
}

fn random_scalar_bytes() -> Vec<u8> {
    PrivateKey::random(&mut rand::rng()).as_bytes().to_vec()
}

/// The varint length prefix borsh uses for scripts, stacks and covenants
fn varint(mut value: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = u8::try_from(value & 0x7f).unwrap();
        value >>= 7;
        if value == 0 {
            bytes.push(byte);
            return bytes;
        }
        bytes.push(byte | 0x80);
    }
}

fn varint_prefixed(bytes: &[u8]) -> Vec<u8> {
    [varint(bytes.len()), bytes.to_vec()].concat()
}

/// The JSON form of an enum value given as a byte: the serialized variant if the byte is known, otherwise a name no
/// variant has.
fn enum_json<T: serde::Serialize>(known: Option<T>, value: u64) -> Value {
    match known {
        Some(variant) => serde_json::to_value(variant).unwrap(),
        None => Value::from(format!("Unknown{value}")),
    }
}

/// The first outputs and kernel of the mainnet genesis block, with a compact input added and the first output's script
/// and covenant replaced, so that every field a sample changes has a unique encoding (needed to splice the binary
/// encodings).
fn base_body() -> AggregateBody {
    let block = get_genesis_block(Network::MainNet).block().clone();
    let (_, mut outputs, mut kernels) = block.body.dissolve();
    outputs.truncate(2);
    kernels.truncate(1);
    let input = TransactionInput::new_with_output_hash(
        FixedHash::zero(),
        ExecutionStack::new(vec![StackItem::PublicKey(random_public_key())]),
        Default::default(),
    );
    // A full input spending the second output, with fields of its own (so that they are unique in the encodings)
    let spent = &outputs[1];
    let mut encrypted_data = vec![0u8; STATIC_ENCRYPTED_DATA_SIZE_TOTAL + 10];
    rand::rng().fill(&mut encrypted_data[..]);
    let random_scalar = || PrivateKey::random(&mut rand::rng());
    let full_input = TransactionInput::new_with_output_data(
        TransactionInputVersion::get_current_version(),
        spent.features.clone(),
        CompressedCommitment::from_canonical_bytes(&random_public_key_bytes()).unwrap(),
        script!(PushPubKey(Box::new(random_public_key()))).unwrap(),
        ExecutionStack::new(vec![StackItem::PublicKey(random_public_key())]),
        Default::default(),
        random_public_key(),
        spent.covenant.clone(),
        EncryptedData::from_bytes(&encrypted_data).unwrap(),
        ComAndPubSignature::new(
            CompressedCommitment::from_canonical_bytes(&random_public_key_bytes()).unwrap(),
            random_public_key(),
            random_scalar(),
            random_scalar(),
            random_scalar(),
        ),
        FixedHash::from(rand::rng().random::<[u8; 32]>()),
        spent.minimum_value_promise,
    );
    let output = outputs.first_mut().unwrap();
    output.script = script!(PushPubKey(Box::new(random_public_key()))).unwrap();
    let hash = FixedHash::from(rand::rng().random::<[u8; 32]>());
    output.covenant = covenant!(output_hash_eq(@hash(hash))).unwrap();
    AggregateBody::new_unsorted(vec![input, full_input], outputs, kernels)
}

fn base_block() -> Block {
    let header = get_genesis_block(Network::MainNet).block().header.clone();
    Block::new(header, base_body())
}

fn transaction_with_body(body: AggregateBody) -> Transaction {
    let (inputs, outputs, kernels) = body.dissolve();
    Transaction::new(
        inputs,
        outputs,
        kernels,
        PrivateKey::random(&mut rand::rng()),
        PrivateKey::random(&mut rand::rng()),
    )
}

/// The base body, with the side-chain feature of an `OutputSideChain` mutation installed on the first output, or a
/// unique coinbase extra for an `OutputCoinbaseExtra` mutation (so that it can be spliced in the binary encodings)
fn prepared_body(body: &AggregateBody, mutation: &Mutation) -> AggregateBody {
    let (inputs, mut outputs, kernels) = body.clone().dissolve();
    match mutation {
        Mutation::OutputSideChain(feature, _) => outputs[0].features.sidechain_feature = Some(*feature.clone()),
        Mutation::OutputCoinbaseExtra(_) => {
            outputs[0].features.coinbase_extra = rand::rng().random::<[u8; 40]>().to_vec().try_into().unwrap()
        },
        _ => {},
    }
    AggregateBody::new_unsorted(inputs, outputs, kernels)
}

/// The JSON pointer (below the side-chain feature) and new value of a side-chain change
fn side_chain_json(feature: &SideChainFeature, change: &SideChainChange) -> Option<(String, Value)> {
    let data = match feature.data {
        SideChainFeatureData::ValidatorNodeRegistration(_) => "/data/ValidatorNodeRegistration",
        SideChainFeatureData::CodeTemplateRegistration(_) => "/data/CodeTemplateRegistration",
        SideChainFeatureData::ConfidentialOutput(_) => "/data/ConfidentialOutput",
        SideChainFeatureData::ValidatorNodeExit(_) => "/data/ValidatorNodeExit",
    };
    let (field, value) = match change {
        SideChainChange::Nothing => return None,
        SideChainChange::DataPublicKey(bytes) => {
            let field = match feature.data {
                SideChainFeatureData::ValidatorNodeRegistration(_) | SideChainFeatureData::ValidatorNodeExit(_) => {
                    "/signature/public_key"
                },
                SideChainFeatureData::CodeTemplateRegistration(_) => "/author_public_key",
                SideChainFeatureData::ConfidentialOutput(_) => "/claim_public_key",
            };
            (format!("{data}{field}"), Value::from(to_hex(bytes)))
        },
        SideChainChange::RegistrationClaimPublicKey(bytes) => {
            (format!("{data}/claim_public_key"), Value::from(to_hex(bytes)))
        },
        SideChainChange::TemplateName(name) => (format!("{data}/template_name/string"), Value::from(name.clone())),
        SideChainChange::TemplateVersion(version) => (format!("{data}/template_version"), Value::from(*version)),
        // `FixedHash` is a JSON array of numbers
        SideChainChange::TemplateBinarySha(bytes) => (format!("{data}/binary_sha"), Value::from(bytes.clone())),
        SideChainChange::TemplateBinaryUrl(url) => (format!("{data}/binary_url/string"), Value::from(url.clone())),
        SideChainChange::TemplateCommitHash(bytes) => (
            format!("{data}/build_info/commit_hash/inner"),
            Value::from(bytes.clone()),
        ),
        SideChainChange::TemplateType(template_type) => (format!("{data}/template_type"), match template_type {
            TemplateTypeSample::Wasm(abi_version) => serde_json::json!({ "Wasm": { "abi_version": abi_version } }),
            TemplateTypeSample::Flow => Value::from("Flow"),
            TemplateTypeSample::Manifest => Value::from("Manifest"),
        }),
        SideChainChange::SidechainIdPublicKey(bytes) => {
            ("/sidechain_id/public_key".to_string(), Value::from(to_hex(bytes)))
        },
        SideChainChange::SidechainIdSignature(bytes) => (
            "/sidechain_id/knowledge_proof/signature".to_string(),
            Value::from(to_hex(bytes)),
        ),
    };
    Some((format!("/body/outputs/0/features/sidechain_feature{field}"), value))
}

#[allow(clippy::too_many_lines)]
fn mutate_json(value: &mut Value, mutation: &Mutation) {
    let hex = |bytes: &Vec<u8>| Value::from(to_hex(bytes));
    let (pointer, new_value) = match mutation {
        Mutation::Nothing => return,
        Mutation::OutputEncryptedData(bytes) => ("/body/outputs/0/encrypted_data/data".to_string(), hex(bytes)),
        Mutation::OutputScript(bytes) => ("/body/outputs/0/script".to_string(), hex(bytes)),
        Mutation::OutputCovenant(bytes) => ("/body/outputs/0/covenant".to_string(), hex(bytes)),
        Mutation::OutputFeaturesVersion(version) => (
            "/body/outputs/0/features/version".to_string(),
            enum_json(
                u8::try_from(*version)
                    .ok()
                    .and_then(|v| OutputFeaturesVersion::try_from(v).ok()),
                u64::from(*version),
            ),
        ),
        // `OutputType` is a JSON number
        Mutation::OutputType(output_type) => (
            "/body/outputs/0/features/output_type".to_string(),
            Value::from(*output_type),
        ),
        Mutation::OutputRangeProofType(range_proof_type) => (
            "/body/outputs/0/features/range_proof_type".to_string(),
            enum_json(
                u8::try_from(*range_proof_type).ok().and_then(RangeProofType::from_byte),
                u64::from(*range_proof_type),
            ),
        ),
        Mutation::OutputCoinbaseExtra(bytes) => ("/body/outputs/0/features/coinbase_extra".to_string(), hex(bytes)),
        Mutation::OutputRangeProof(bytes) => ("/body/outputs/0/proof".to_string(), hex(bytes)),
        Mutation::OutputCommitment(bytes) => ("/body/outputs/0/commitment".to_string(), hex(bytes)),
        Mutation::OutputSenderOffsetPublicKey(bytes) => {
            ("/body/outputs/0/sender_offset_public_key".to_string(), hex(bytes))
        },
        Mutation::OutputMetadataSignatureUa(bytes) => {
            ("/body/outputs/0/metadata_signature/u_a".to_string(), hex(bytes))
        },
        Mutation::OutputSideChain(feature, change) => match side_chain_json(feature, change) {
            Some(pointer_and_value) => pointer_and_value,
            None => return,
        },
        Mutation::InputData(bytes) => ("/body/inputs/0/input_data".to_string(), hex(bytes)),
        Mutation::InputScriptSignatureEphemeralPubkey(bytes) => (
            "/body/inputs/0/script_signature/ephemeral_pubkey".to_string(),
            hex(bytes),
        ),
        Mutation::FullInputScript(bytes) => ("/body/inputs/1/spent_output/OutputData/script".to_string(), hex(bytes)),
        Mutation::FullInputCommitment(bytes) => (
            "/body/inputs/1/spent_output/OutputData/commitment".to_string(),
            hex(bytes),
        ),
        Mutation::FullInputSenderOffsetPublicKey(bytes) => (
            "/body/inputs/1/spent_output/OutputData/sender_offset_public_key".to_string(),
            hex(bytes),
        ),
        Mutation::FullInputMetadataSignatureUa(bytes) => (
            "/body/inputs/1/spent_output/OutputData/metadata_signature/u_a".to_string(),
            hex(bytes),
        ),
        Mutation::FullInputOutputType(output_type) => (
            "/body/inputs/1/spent_output/OutputData/features/output_type".to_string(),
            Value::from(*output_type),
        ),
        // `FixedHash` is a JSON array of numbers
        Mutation::FullInputRangeproofHash(bytes) => (
            "/body/inputs/1/spent_output/OutputData/rangeproof_hash".to_string(),
            Value::from(bytes.clone()),
        ),
        Mutation::InputVersion(version) => (
            "/body/inputs/1/version".to_string(),
            enum_json(
                u8::try_from(*version)
                    .ok()
                    .and_then(|v| TransactionInputVersion::try_from(v).ok()),
                u64::from(*version),
            ),
        ),
        Mutation::OutputVersion(version) => (
            "/body/outputs/0/version".to_string(),
            enum_json(
                u8::try_from(*version)
                    .ok()
                    .and_then(|v| TransactionOutputVersion::try_from(v).ok()),
                u64::from(*version),
            ),
        ),
        Mutation::KernelVersion(version) => (
            "/body/kernels/0/version".to_string(),
            enum_json(
                u8::try_from(*version)
                    .ok()
                    .and_then(|v| TransactionKernelVersion::try_from(v).ok()),
                u64::from(*version),
            ),
        ),
        Mutation::KernelBurnCommitment(bytes) => ("/body/kernels/0/burn_commitment".to_string(), hex(bytes)),
        // `FixedHash` is a JSON array of numbers
        Mutation::HeaderHash(field, bytes) => {
            let name = match field {
                HeaderHash::PrevHash => "prev_hash",
                HeaderHash::OutputMr => "output_mr",
                HeaderHash::KernelMr => "kernel_mr",
                HeaderHash::InputMr => "input_mr",
                HeaderHash::ValidatorNodeMr => "validator_node_mr",
            };
            (format!("/header/{name}"), Value::from(bytes.clone()))
        },
        Mutation::FullInputEncryptedData(bytes) => (
            "/body/inputs/1/spent_output/OutputData/encrypted_data/data".to_string(),
            hex(bytes),
        ),
        Mutation::KernelFeatures(bits) => ("/body/kernels/0/features".to_string(), Value::from(*bits)),
        Mutation::KernelExcess(bytes) => ("/body/kernels/0/excess".to_string(), hex(bytes)),
        Mutation::KernelExcessSignature(bytes) => ("/body/kernels/0/excess_sig/signature".to_string(), hex(bytes)),
        // `FixedHash` is a JSON array of numbers
        Mutation::HeaderBlockOutputMr(bytes) => ("/header/block_output_mr".to_string(), Value::from(bytes.clone())),
        Mutation::HeaderVersion(version) => ("/header/version".to_string(), Value::from(*version)),
        Mutation::HeaderTotalKernelOffset(bytes) => ("/header/total_kernel_offset".to_string(), hex(bytes)),
        Mutation::HeaderPowAlgo(algo) => (
            "/header/pow/pow_algo".to_string(),
            enum_json(PowAlgorithm::try_from(*algo).ok(), *algo),
        ),
        Mutation::HeaderPowData(bytes) => ("/header/pow/pow_data".to_string(), hex(bytes)),
        Mutation::TransactionOffset(bytes) => ("/offset".to_string(), hex(bytes)),
    };
    assert!(value.pointer(&pointer).is_some(), "{pointer} not found in {value}");
    *value.pointer_mut(&pointer).unwrap() = new_value;
}

fn mutate_p2p_side_chain(feature: &mut proto::types::SideChainFeature, change: &SideChainChange) {
    use proto::types::side_chain_feature::SideChainFeature as Data;
    let data = feature.side_chain_feature.as_mut().unwrap();
    match (change, data) {
        (SideChainChange::Nothing, _) => {},
        (SideChainChange::DataPublicKey(bytes), Data::ValidatorNodeRegistration(reg)) => reg.public_key = bytes.clone(),
        (SideChainChange::DataPublicKey(bytes), Data::ValidatorNodeExit(exit)) => exit.public_key = bytes.clone(),
        (SideChainChange::DataPublicKey(bytes), Data::TemplateRegistration(reg)) => {
            reg.author_public_key = bytes.clone()
        },
        (SideChainChange::DataPublicKey(bytes), Data::ConfidentialOutput(output)) => {
            output.claim_public_key = bytes.clone()
        },
        (SideChainChange::RegistrationClaimPublicKey(bytes), Data::ValidatorNodeRegistration(reg)) => {
            reg.claim_public_key = bytes.clone()
        },
        (SideChainChange::TemplateName(name), Data::TemplateRegistration(reg)) => reg.template_name = name.clone(),
        (SideChainChange::TemplateVersion(version), Data::TemplateRegistration(reg)) => reg.template_version = *version,
        (SideChainChange::TemplateBinarySha(bytes), Data::TemplateRegistration(reg)) => reg.binary_sha = bytes.clone(),
        (SideChainChange::TemplateBinaryUrl(url), Data::TemplateRegistration(reg)) => reg.binary_url = url.clone(),
        (SideChainChange::TemplateCommitHash(bytes), Data::TemplateRegistration(reg)) => {
            reg.build_info.as_mut().unwrap().commit_hash = bytes.clone()
        },
        (SideChainChange::TemplateType(template_type), Data::TemplateRegistration(reg)) => {
            use proto::types::template_type::TemplateType as Type;
            reg.template_type = Some(proto::types::TemplateType {
                template_type: Some(match template_type {
                    TemplateTypeSample::Wasm(abi_version) => Type::Wasm(proto::types::WasmInfo {
                        abi_version: *abi_version,
                    }),
                    TemplateTypeSample::Flow => Type::Flow(proto::types::FlowInfo {}),
                    TemplateTypeSample::Manifest => Type::Manifest(proto::types::ManifestInfo {}),
                }),
            });
        },
        (SideChainChange::SidechainIdPublicKey(bytes), _) => {
            feature.sidechain_id.as_mut().unwrap().public_key = bytes.clone()
        },
        (SideChainChange::SidechainIdSignature(bytes), _) => {
            feature
                .sidechain_id
                .as_mut()
                .unwrap()
                .knowledge_proof
                .as_mut()
                .unwrap()
                .signature = bytes.clone()
        },
        (change, _) => panic!("{change:?} does not apply to this side-chain feature"),
    }
}

fn mutate_grpc_side_chain(feature: &mut grpc::SideChainFeature, change: &SideChainChange) {
    use grpc::side_chain_feature::Feature as Data;
    let data = feature.feature.as_mut().unwrap();
    match (change, data) {
        (SideChainChange::Nothing, _) => {},
        (SideChainChange::DataPublicKey(bytes), Data::ValidatorNodeRegistration(reg)) => reg.public_key = bytes.clone(),
        (SideChainChange::DataPublicKey(bytes), Data::ValidatorNodeExit(exit)) => exit.public_key = bytes.clone(),
        (SideChainChange::DataPublicKey(bytes), Data::TemplateRegistration(reg)) => {
            reg.author_public_key = bytes.clone()
        },
        (SideChainChange::DataPublicKey(bytes), Data::ConfidentialOutput(output)) => {
            output.claim_public_key = bytes.clone()
        },
        (SideChainChange::RegistrationClaimPublicKey(bytes), Data::ValidatorNodeRegistration(reg)) => {
            reg.claim_public_key = bytes.clone()
        },
        (SideChainChange::TemplateName(name), Data::TemplateRegistration(reg)) => reg.template_name = name.clone(),
        (SideChainChange::TemplateVersion(version), Data::TemplateRegistration(reg)) => reg.template_version = *version,
        (SideChainChange::TemplateBinarySha(bytes), Data::TemplateRegistration(reg)) => reg.binary_sha = bytes.clone(),
        (SideChainChange::TemplateBinaryUrl(url), Data::TemplateRegistration(reg)) => reg.binary_url = url.clone(),
        (SideChainChange::TemplateCommitHash(bytes), Data::TemplateRegistration(reg)) => {
            reg.build_info.as_mut().unwrap().commit_hash = bytes.clone()
        },
        (SideChainChange::TemplateType(template_type), Data::TemplateRegistration(reg)) => {
            use grpc::template_type::TemplateType as Type;
            reg.template_type = Some(grpc::TemplateType {
                template_type: Some(match template_type {
                    TemplateTypeSample::Wasm(abi_version) => Type::Wasm(grpc::WasmInfo {
                        abi_version: *abi_version,
                    }),
                    TemplateTypeSample::Flow => Type::Flow(grpc::FlowInfo {}),
                    TemplateTypeSample::Manifest => Type::Manifest(grpc::ManifestInfo {}),
                }),
            });
        },
        (SideChainChange::SidechainIdPublicKey(bytes), _) => {
            feature.sidechain_id.as_mut().unwrap().public_key = bytes.clone()
        },
        (SideChainChange::SidechainIdSignature(bytes), _) => {
            feature
                .sidechain_id
                .as_mut()
                .unwrap()
                .knowledge_proof
                .as_mut()
                .unwrap()
                .signature = bytes.clone()
        },
        (change, _) => panic!("{change:?} does not apply to this side-chain feature"),
    }
}

fn mutate_p2p_body(body: &mut proto::types::AggregateBody, mutation: &Mutation) {
    let output = &mut body.outputs[0];
    let [input, full_input] = &mut body.inputs[..] else {
        panic!("the base body has a compact and a full input");
    };
    let kernel = &mut body.kernels[0];
    match mutation {
        Mutation::OutputEncryptedData(bytes) => output.encrypted_data = bytes.clone(),
        Mutation::OutputScript(bytes) => output.script = bytes.clone(),
        // The covenant field holds the borsh encoding of the covenant
        Mutation::OutputCovenant(bytes) => output.covenant = varint_prefixed(bytes),
        Mutation::OutputFeaturesVersion(version) => output.features.as_mut().unwrap().version = *version,
        Mutation::OutputType(output_type) => output.features.as_mut().unwrap().output_type = *output_type,
        Mutation::OutputRangeProofType(range_proof_type) => {
            output.features.as_mut().unwrap().range_proof_type = *range_proof_type
        },
        Mutation::OutputCoinbaseExtra(bytes) => output.features.as_mut().unwrap().coinbase_extra = bytes.clone(),
        Mutation::OutputRangeProof(bytes) => output.range_proof.as_mut().unwrap().proof_bytes = bytes.clone(),
        Mutation::OutputCommitment(bytes) => output.commitment.as_mut().unwrap().data = bytes.clone(),
        Mutation::OutputSenderOffsetPublicKey(bytes) => output.sender_offset_public_key = bytes.clone(),
        Mutation::OutputMetadataSignatureUa(bytes) => output.metadata_signature.as_mut().unwrap().u_a = bytes.clone(),
        Mutation::OutputSideChain(_, change) => mutate_p2p_side_chain(
            output.features.as_mut().unwrap().sidechain_feature.as_mut().unwrap(),
            change,
        ),
        Mutation::InputData(bytes) => input.input_data = bytes.clone(),
        Mutation::InputScriptSignatureEphemeralPubkey(bytes) => {
            input.script_signature.as_mut().unwrap().ephemeral_pubkey = bytes.clone()
        },
        Mutation::FullInputScript(bytes) => full_input.script = bytes.clone(),
        Mutation::FullInputEncryptedData(bytes) => full_input.encrypted_data = bytes.clone(),
        Mutation::FullInputCommitment(bytes) => full_input.commitment.as_mut().unwrap().data = bytes.clone(),
        Mutation::FullInputSenderOffsetPublicKey(bytes) => full_input.sender_offset_public_key = bytes.clone(),
        Mutation::FullInputMetadataSignatureUa(bytes) => {
            full_input.metadata_signature.as_mut().unwrap().u_a = bytes.clone()
        },
        Mutation::FullInputOutputType(output_type) => full_input.features.as_mut().unwrap().output_type = *output_type,
        Mutation::FullInputRangeproofHash(bytes) => full_input.rangeproof_hash = bytes.clone(),
        Mutation::InputVersion(version) => full_input.version = *version,
        Mutation::OutputVersion(version) => output.version = *version,
        Mutation::KernelVersion(version) => kernel.version = *version,
        Mutation::KernelBurnCommitment(bytes) => {
            kernel.burn_commitment = Some(proto::types::Commitment { data: bytes.clone() })
        },
        Mutation::KernelFeatures(bits) => kernel.features = *bits,
        Mutation::KernelExcess(bytes) => kernel.excess.as_mut().unwrap().data = bytes.clone(),
        Mutation::KernelExcessSignature(bytes) => kernel.excess_sig.as_mut().unwrap().signature = bytes.clone(),
        _ => {},
    }
}

fn mutate_grpc_body(body: &mut grpc::AggregateBody, mutation: &Mutation) {
    let output = &mut body.outputs[0];
    let [input, full_input] = &mut body.inputs[..] else {
        panic!("the base body has a compact and a full input");
    };
    let kernel = &mut body.kernels[0];
    match mutation {
        Mutation::OutputEncryptedData(bytes) => output.encrypted_data = bytes.clone(),
        Mutation::OutputScript(bytes) => output.script = bytes.clone(),
        // The covenant field holds the borsh encoding of the covenant
        Mutation::OutputCovenant(bytes) => output.covenant = varint_prefixed(bytes),
        Mutation::OutputFeaturesVersion(version) => output.features.as_mut().unwrap().version = *version,
        Mutation::OutputType(output_type) => output.features.as_mut().unwrap().output_type = *output_type,
        Mutation::OutputRangeProofType(range_proof_type) => {
            output.features.as_mut().unwrap().range_proof_type = *range_proof_type
        },
        Mutation::OutputCoinbaseExtra(bytes) => output.features.as_mut().unwrap().coinbase_extra = bytes.clone(),
        Mutation::OutputRangeProof(bytes) => output.range_proof.as_mut().unwrap().proof_bytes = bytes.clone(),
        Mutation::OutputCommitment(bytes) => output.commitment = bytes.clone(),
        Mutation::OutputSenderOffsetPublicKey(bytes) => output.sender_offset_public_key = bytes.clone(),
        Mutation::OutputMetadataSignatureUa(bytes) => output.metadata_signature.as_mut().unwrap().u_a = bytes.clone(),
        Mutation::OutputSideChain(_, change) => mutate_grpc_side_chain(
            output.features.as_mut().unwrap().sidechain_feature.as_mut().unwrap(),
            change,
        ),
        Mutation::InputData(bytes) => input.input_data = bytes.clone(),
        Mutation::InputScriptSignatureEphemeralPubkey(bytes) => {
            input.script_signature.as_mut().unwrap().ephemeral_pubkey = bytes.clone()
        },
        Mutation::FullInputScript(bytes) => full_input.script = bytes.clone(),
        Mutation::FullInputEncryptedData(bytes) => full_input.encrypted_data = bytes.clone(),
        Mutation::FullInputCommitment(bytes) => full_input.commitment = bytes.clone(),
        Mutation::FullInputSenderOffsetPublicKey(bytes) => full_input.sender_offset_public_key = bytes.clone(),
        Mutation::FullInputMetadataSignatureUa(bytes) => {
            full_input.metadata_signature.as_mut().unwrap().u_a = bytes.clone()
        },
        Mutation::FullInputOutputType(output_type) => full_input.features.as_mut().unwrap().output_type = *output_type,
        Mutation::FullInputRangeproofHash(bytes) => full_input.rangeproof_hash = bytes.clone(),
        Mutation::InputVersion(version) => full_input.version = *version,
        Mutation::OutputVersion(version) => output.version = *version,
        Mutation::KernelVersion(version) => kernel.version = *version,
        Mutation::KernelBurnCommitment(bytes) => kernel.burn_commitment = bytes.clone(),
        Mutation::KernelFeatures(bits) => kernel.features = *bits,
        Mutation::KernelExcess(bytes) => kernel.excess = bytes.clone(),
        Mutation::KernelExcessSignature(bytes) => kernel.excess_sig.as_mut().unwrap().signature = bytes.clone(),
        _ => {},
    }
}

/// The encodings of a changed field before and after the change: `(borsh_before, borsh_after, bincode_before,
/// bincode_after)`
type BinaryMutation = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

/// A same-size change of a fixed size field (a key, commitment, hash or scalar), which appears as its raw bytes in
/// both binary formats. `None` if the new value has a different size, which the binary formats can not express.
fn same_size(before: &[u8], after: &[u8]) -> Option<BinaryMutation> {
    if before.len() != after.len() {
        return None;
    }
    Some((before.to_vec(), after.to_vec(), before.to_vec(), after.to_vec()))
}

/// A change of a length-prefixed byte or string field whose borsh and bincode encodings are those of a `Vec<u8>` (a
/// `u32` and a `u64` length prefix): checks that `field` is encoded that way, then splices `after` in for `before`.
fn byte_vec_splice<T: BorshSerialize + serde::Serialize>(field: &T, before: &[u8], after: &[u8]) -> BinaryMutation {
    assert_eq!(borsh::to_vec(field).unwrap(), borsh::to_vec(&before.to_vec()).unwrap());
    assert_eq!(
        bincode::serialize(field).unwrap(),
        bincode::serialize(&before.to_vec()).unwrap()
    );
    (
        borsh::to_vec(&before.to_vec()).unwrap(),
        borsh::to_vec(&after.to_vec()).unwrap(),
        bincode::serialize(&before.to_vec()).unwrap(),
        bincode::serialize(&after.to_vec()).unwrap(),
    )
}

/// A change of a script, stack or covenant field: a varint length prefix in borsh, a `u64` one in bincode
fn varint_splice<T: BorshSerialize + serde::Serialize>(field: &T, after: &[u8]) -> BinaryMutation {
    (
        borsh::to_vec(field).unwrap(),
        varint_prefixed(after),
        bincode::serialize(field).unwrap(),
        bincode::serialize(&after.to_vec()).unwrap(),
    )
}

/// A change of the version of an output, input or kernel, its first field: replaces the leading version bytes of
/// `item`'s encodings with `value` (little endian, at the width each format uses for `version`). `None` if the value
/// does not fit in a byte.
fn version_splice<T, V>(item: &T, version: &V, value: u32) -> Option<BinaryMutation>
where
    T: BorshSerialize + serde::Serialize,
    V: BorshSerialize + serde::Serialize,
{
    let value = u8::try_from(value).ok()?;
    let overwrite_prefix = |encoding: Vec<u8>, width: usize| {
        let mut changed = encoding.clone();
        changed[..width].copy_from_slice(&u64::from(value).to_le_bytes()[..width]);
        (encoding, changed)
    };
    let (borsh_before, borsh_after) =
        overwrite_prefix(borsh::to_vec(item).unwrap(), borsh::to_vec(version).unwrap().len());
    let (bincode_before, bincode_after) = overwrite_prefix(
        bincode::serialize(item).unwrap(),
        bincode::serialize(version).unwrap().len(),
    );
    Some((borsh_before, borsh_after, bincode_before, bincode_after))
}

fn side_chain_binary_mutation(feature: &SideChainFeature, change: &SideChainChange) -> Option<BinaryMutation> {
    let data_key = match &feature.data {
        SideChainFeatureData::ValidatorNodeRegistration(reg) => reg.public_key(),
        SideChainFeatureData::ValidatorNodeExit(exit) => exit.public_key(),
        SideChainFeatureData::CodeTemplateRegistration(reg) => &reg.author_public_key,
        SideChainFeatureData::ConfidentialOutput(output) => &output.claim_public_key,
    };
    let template = match &feature.data {
        SideChainFeatureData::CodeTemplateRegistration(reg) => Some(reg),
        _ => None,
    };
    match change {
        SideChainChange::Nothing => Some(Default::default()),
        SideChainChange::DataPublicKey(bytes) => same_size(data_key.as_bytes(), bytes),
        SideChainChange::RegistrationClaimPublicKey(bytes) => match &feature.data {
            SideChainFeatureData::ValidatorNodeRegistration(reg) => same_size(reg.claim_public_key().as_bytes(), bytes),
            _ => None,
        },
        SideChainChange::SidechainIdPublicKey(bytes) => {
            same_size(feature.sidechain_id.as_ref()?.public_key().as_bytes(), bytes)
        },
        SideChainChange::SidechainIdSignature(bytes) => same_size(
            feature
                .sidechain_id
                .as_ref()?
                .knowledge_proof()
                .get_signature()
                .as_bytes(),
            bytes,
        ),
        SideChainChange::TemplateName(name) => {
            let field = &template?.template_name;
            Some(byte_vec_splice(field, field.as_str().as_bytes(), name.as_bytes()))
        },
        SideChainChange::TemplateBinaryUrl(url) => {
            let field = &template?.binary_url;
            Some(byte_vec_splice(field, field.as_str().as_bytes(), url.as_bytes()))
        },
        SideChainChange::TemplateCommitHash(bytes) => {
            let field = &template?.build_info.commit_hash;
            Some(byte_vec_splice(field, field.as_bytes(), bytes))
        },
        SideChainChange::TemplateBinarySha(bytes) => same_size(template?.binary_sha.as_slice(), bytes),
        // Built directly, see `direct_body`
        SideChainChange::TemplateType(_) => None,
        // Written with `overwrite`
        SideChainChange::TemplateVersion(_) => None,
    }
}

/// The [`BinaryMutation`] of a change to a body, header or transaction field that can be spliced. `None` if the
/// binary formats can not express the change (a fixed size field of the wrong size), or it is written with
/// [`overwrite`] instead.
fn binary_mutation(
    body: &AggregateBody,
    header: Option<&BlockHeader>,
    offset: Option<&PrivateKey>,
    mutation: &Mutation,
) -> Option<BinaryMutation> {
    let output = body.outputs().first().unwrap();
    let input = body.inputs().first().unwrap();
    let full_input = body.inputs().get(1).unwrap();
    let kernel = body.kernels().first().unwrap();
    match mutation {
        Mutation::Nothing => Some(Default::default()),
        Mutation::OutputEncryptedData(bytes) => Some(byte_vec_splice(
            &output.encrypted_data,
            output.encrypted_data.as_bytes(),
            bytes,
        )),
        Mutation::FullInputEncryptedData(bytes) => {
            let field = full_input.encrypted_data().unwrap();
            Some(byte_vec_splice(field, field.as_bytes(), bytes))
        },
        Mutation::OutputScript(bytes) => Some(varint_splice(&output.script, bytes)),
        Mutation::FullInputScript(bytes) => Some(varint_splice(full_input.script().unwrap(), bytes)),
        Mutation::OutputCovenant(bytes) => Some(varint_splice(&output.covenant, bytes)),
        Mutation::InputData(bytes) => Some(varint_splice(&input.input_data, bytes)),
        Mutation::OutputCoinbaseExtra(bytes) => Some(byte_vec_splice(
            &output.features.coinbase_extra,
            output.features.coinbase_extra.as_bytes(),
            bytes,
        )),
        Mutation::OutputRangeProof(bytes) => {
            let proof = output.proof.as_ref().unwrap();
            Some(byte_vec_splice(proof, proof.as_bytes(), bytes))
        },
        Mutation::OutputCommitment(bytes) => same_size(output.commitment.as_bytes(), bytes),
        Mutation::OutputSenderOffsetPublicKey(bytes) => same_size(output.sender_offset_public_key.as_bytes(), bytes),
        Mutation::OutputMetadataSignatureUa(bytes) => same_size(output.metadata_signature.u_a().as_bytes(), bytes),
        Mutation::OutputSideChain(feature, change) => side_chain_binary_mutation(feature, change),
        Mutation::KernelExcess(bytes) => same_size(kernel.excess.as_bytes(), bytes),
        Mutation::KernelExcessSignature(bytes) => same_size(kernel.excess_sig.get_signature().as_bytes(), bytes),
        Mutation::HeaderBlockOutputMr(bytes) => same_size(header?.block_output_mr.as_slice(), bytes),
        Mutation::HeaderTotalKernelOffset(bytes) => same_size(header?.total_kernel_offset.as_bytes(), bytes),
        Mutation::HeaderPowData(bytes) => {
            let pow_data = &header?.pow.pow_data;
            Some(byte_vec_splice(pow_data, pow_data.as_bytes(), bytes))
        },
        Mutation::TransactionOffset(bytes) => same_size(offset?.as_bytes(), bytes),
        Mutation::FullInputCommitment(bytes) => same_size(full_input.commitment().unwrap().as_bytes(), bytes),
        Mutation::FullInputSenderOffsetPublicKey(bytes) => {
            same_size(full_input.sender_offset_public_key().unwrap().as_bytes(), bytes)
        },
        Mutation::FullInputMetadataSignatureUa(bytes) => {
            same_size(full_input.metadata_signature().unwrap().u_a().as_bytes(), bytes)
        },
        Mutation::FullInputRangeproofHash(bytes) => same_size(full_input.rangeproof_hash().unwrap().as_slice(), bytes),
        Mutation::InputVersion(value) => version_splice(full_input, &full_input.version, *value),
        Mutation::OutputVersion(value) => version_splice(output, &output.version, *value),
        Mutation::KernelVersion(value) => version_splice(kernel, &kernel.version, *value),
        // Built directly, see `direct_body` and `check_block`
        Mutation::KernelBurnCommitment(_) | Mutation::HeaderHash(..) => None,
        // The script signature of the compact input is the default (all zero), which is not unique in the encoding
        Mutation::InputScriptSignatureEphemeralPubkey(_) => None,
        // Written with `overwrite`
        Mutation::OutputFeaturesVersion(_) |
        Mutation::OutputType(_) |
        Mutation::FullInputOutputType(_) |
        Mutation::OutputRangeProofType(_) |
        Mutation::KernelFeatures(_) |
        Mutation::HeaderVersion(_) |
        Mutation::HeaderPowAlgo(_) => None,
    }
}

/// For a change to a fixed width integer or enum field: the value to write and its width in borsh and in bincode
fn overwrite_spec(mutation: &Mutation) -> Option<(u64, usize, usize)> {
    let width = |borsh: Vec<u8>, bincode: Vec<u8>| (borsh.len(), bincode.len());
    let (value, (borsh_width, bincode_width)) = match mutation {
        Mutation::OutputFeaturesVersion(v) => (
            u64::from(*v),
            width(
                borsh::to_vec(&OutputFeaturesVersion::V0).unwrap(),
                bincode::serialize(&OutputFeaturesVersion::V0).unwrap(),
            ),
        ),
        Mutation::OutputType(v) | Mutation::FullInputOutputType(v) => (
            u64::from(*v),
            width(
                borsh::to_vec(&OutputType::Standard).unwrap(),
                bincode::serialize(&OutputType::Standard).unwrap(),
            ),
        ),
        Mutation::OutputRangeProofType(v) => (
            u64::from(*v),
            width(
                borsh::to_vec(&RangeProofType::BulletProofPlus).unwrap(),
                bincode::serialize(&RangeProofType::BulletProofPlus).unwrap(),
            ),
        ),
        Mutation::KernelFeatures(v) => (
            u64::from(*v),
            width(
                borsh::to_vec(&KernelFeatures::empty()).unwrap(),
                bincode::serialize(&KernelFeatures::empty()).unwrap(),
            ),
        ),
        Mutation::HeaderVersion(v) | Mutation::OutputSideChain(_, SideChainChange::TemplateVersion(v)) => {
            (u64::from(*v), (2, 2))
        },
        Mutation::HeaderPowAlgo(v) => (
            *v,
            width(
                borsh::to_vec(&PowAlgorithm::Sha3x).unwrap(),
                bincode::serialize(&PowAlgorithm::Sha3x).unwrap(),
            ),
        ),
        _ => return None,
    };
    Some((value, borsh_width, bincode_width))
}

/// The body with the field an `overwrite` mutation changes set to another valid value, differing in its lowest byte
fn alternative_body(body: &AggregateBody, mutation: &Mutation) -> AggregateBody {
    let (mut inputs, mut outputs, mut kernels) = body.clone().dissolve();
    if let Mutation::FullInputOutputType(_) = mutation &&
        let SpentOutput::OutputData { features, .. } = &mut inputs[1].spent_output
    {
        features.output_type = OutputType::from_byte(features.output_type.as_byte() ^ 1).unwrap();
    }
    let features = &mut outputs[0].features;
    match mutation {
        Mutation::OutputFeaturesVersion(_) => {
            features.version = OutputFeaturesVersion::try_from(features.version as u8 ^ 1).unwrap()
        },
        Mutation::OutputType(_) => {
            features.output_type = OutputType::from_byte(features.output_type.as_byte() ^ 1).unwrap()
        },
        Mutation::OutputRangeProofType(_) => {
            features.range_proof_type = RangeProofType::from_byte(features.range_proof_type.as_byte() ^ 1).unwrap()
        },
        Mutation::KernelFeatures(_) => {
            kernels[0].features = KernelFeatures::from_bits(kernels[0].features.bits() ^ 1).unwrap()
        },
        Mutation::OutputSideChain(_, SideChainChange::TemplateVersion(_)) => {
            let Some(SideChainFeatureData::CodeTemplateRegistration(reg)) =
                features.sidechain_feature.as_mut().map(|f| &mut f.data)
            else {
                panic!("a template version change needs a template registration");
            };
            reg.template_version ^= 1;
        },
        _ => {},
    }
    AggregateBody::new_unsorted(inputs, outputs, kernels)
}

/// The body with the change applied directly, for changes the binary formats can express but that can not be spliced
/// (the field changes size, as an absent burn commitment becoming present, or is not unique in the encoding). `None`
/// for other changes, and for values the Rust type can not hold.
fn direct_body(body: &AggregateBody, mutation: &Mutation) -> Option<AggregateBody> {
    let (inputs, mut outputs, mut kernels) = body.clone().dissolve();
    match mutation {
        Mutation::KernelBurnCommitment(bytes) => {
            if bytes.len() != 32 {
                return None;
            }
            kernels[0].burn_commitment = Some(CompressedCommitment::from_canonical_bytes(bytes).ok()?);
        },
        Mutation::OutputSideChain(_, SideChainChange::TemplateType(template_type)) => {
            let Some(SideChainFeatureData::CodeTemplateRegistration(reg)) =
                outputs[0].features.sidechain_feature.as_mut().map(|f| &mut f.data)
            else {
                panic!("a template type change needs a template registration");
            };
            reg.template_type = match template_type {
                TemplateTypeSample::Wasm(abi_version) => TemplateType::Wasm {
                    abi_version: u16::try_from(*abi_version).ok()?,
                },
                TemplateTypeSample::Flow => TemplateType::Flow,
                TemplateTypeSample::Manifest => TemplateType::Manifest,
            };
        },
        _ => return None,
    }
    Some(AggregateBody::new_unsorted(inputs, outputs, kernels))
}

/// The header with a `HeaderHash` change applied directly (the genesis values of these hashes are not unique in the
/// encoding, so they can not be spliced). `None` for other changes and for values of the wrong size.
fn direct_header(header: &BlockHeader, mutation: &Mutation) -> Option<BlockHeader> {
    let Mutation::HeaderHash(field, bytes) = mutation else {
        return None;
    };
    let hash = FixedHash::try_from(bytes.as_slice()).ok()?;
    let mut header = header.clone();
    match field {
        HeaderHash::PrevHash => header.prev_hash = hash,
        HeaderHash::OutputMr => header.output_mr = hash,
        HeaderHash::KernelMr => header.kernel_mr = hash,
        HeaderHash::InputMr => header.input_mr = hash,
        HeaderHash::ValidatorNodeMr => header.validator_node_mr = hash,
    }
    Some(header)
}

/// Writes `value` (little endian, `width` bytes) over a fixed width field of `encoding`. The field is found by
/// comparing `encoding` with `alternative`, an encoding of the same container with another value of the field that
/// differs in its lowest byte. `None` if the value does not fit in the field.
fn overwrite(encoding: &[u8], alternative: &[u8], width: usize, value: u64) -> Option<Vec<u8>> {
    if width < 8 && value >> (8 * width) != 0 {
        return None;
    }
    assert_eq!(encoding.len(), alternative.len());
    let differences = (0..encoding.len())
        .filter(|i| encoding[*i] != alternative[*i])
        .collect::<Vec<_>>();
    let start = *differences.first().expect("the alternative differs");
    assert!(
        differences.iter().all(|i| *i < start + width),
        "the field is {width} bytes wide"
    );
    let mut result = encoding.to_vec();
    result[start..start + width].copy_from_slice(&value.to_le_bytes()[..width]);
    Some(result)
}

/// Replaces the single occurrence of `before` in `encoding` with `after`
fn splice(encoding: &[u8], before: &[u8], after: &[u8]) -> Vec<u8> {
    if before.is_empty() {
        return encoding.to_vec();
    }
    let positions = encoding
        .windows(before.len())
        .enumerate()
        .filter(|(_, window)| *window == before)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 1, "the changed field must have a unique encoding");
    [
        &encoding[..positions[0]],
        after,
        &encoding[positions[0] + before.len()..],
    ]
    .concat()
}

/// The borsh and bincode encodings of the changed container, each `None` if that format can not express the change
fn binary_encodings(
    borsh_encoding: Option<Vec<u8>>,
    bincode_encoding: Vec<u8>,
    spliced: Option<BinaryMutation>,
    alternative: Option<(Option<Vec<u8>>, Vec<u8>)>,
    direct: Option<(Option<Vec<u8>>, Vec<u8>)>,
    mutation: &Mutation,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    if let Some((borsh_direct, bincode_direct)) = direct {
        return (borsh_direct, Some(bincode_direct));
    }
    if let Some((borsh_before, borsh_after, bincode_before, bincode_after)) = spliced {
        return (
            borsh_encoding.map(|e| splice(&e, &borsh_before, &borsh_after)),
            Some(splice(&bincode_encoding, &bincode_before, &bincode_after)),
        );
    }
    if let (Some((value, borsh_width, bincode_width)), Some((borsh_alternative, bincode_alternative))) =
        (overwrite_spec(mutation), alternative)
    {
        return (
            borsh_encoding
                .zip(borsh_alternative)
                .and_then(|(e, a)| overwrite(&e, &a, borsh_width, value)),
            overwrite(&bincode_encoding, &bincode_alternative, bincode_width, value),
        );
    }
    (None, None)
}

fn check_block(base: &Block, sample: &Sample) {
    let Sample { name, mutation, accept } = sample;
    let mutation = &mutation;
    let sample = format!("block {name}");
    let block = Block::new(base.header.clone(), prepared_body(&base.body, mutation));

    let mut json = serde_json::to_value(&block).unwrap();
    mutate_json(&mut json, mutation);
    let mut p2p = proto::core::Block::try_from(block.clone()).unwrap();
    mutate_p2p_body(p2p.body.as_mut().unwrap(), mutation);
    let mut grpc_block = grpc::Block::try_from(block.clone()).unwrap();
    mutate_grpc_body(grpc_block.body.as_mut().unwrap(), mutation);
    let p2p_header = p2p.header.as_mut().unwrap();
    let grpc_header = grpc_block.header.as_mut().unwrap();
    let mut alternative = Block::new(block.header.clone(), alternative_body(&block.body, mutation));
    match mutation {
        Mutation::HeaderBlockOutputMr(bytes) => {
            p2p_header.block_output_mr = bytes.clone();
            grpc_header.block_output_mr = bytes.clone();
        },
        Mutation::HeaderHash(field, bytes) => match field {
            HeaderHash::PrevHash => {
                p2p_header.prev_hash = bytes.clone();
                grpc_header.prev_hash = bytes.clone();
            },
            HeaderHash::OutputMr => {
                p2p_header.output_mr = bytes.clone();
                grpc_header.output_mr = bytes.clone();
            },
            HeaderHash::KernelMr => {
                p2p_header.kernel_mr = bytes.clone();
                grpc_header.kernel_mr = bytes.clone();
            },
            HeaderHash::InputMr => {
                p2p_header.input_mr = bytes.clone();
                grpc_header.input_mr = bytes.clone();
            },
            HeaderHash::ValidatorNodeMr => {
                p2p_header.validator_node_merkle_root = bytes.clone();
                grpc_header.validator_node_mr = bytes.clone();
            },
        },
        Mutation::HeaderVersion(version) => {
            p2p_header.version = *version;
            grpc_header.version = *version;
            alternative.header.version ^= 1;
        },
        Mutation::HeaderTotalKernelOffset(bytes) => {
            p2p_header.total_kernel_offset = bytes.clone();
            grpc_header.total_kernel_offset = bytes.clone();
        },
        Mutation::HeaderPowAlgo(algo) => {
            p2p_header.pow.as_mut().unwrap().pow_algo = *algo;
            grpc_header.pow.as_mut().unwrap().pow_algo = *algo;
            let current = block.header.pow.pow_algo.as_u64();
            alternative.header.pow.pow_algo = PowAlgorithm::try_from(current ^ 1).unwrap();
        },
        Mutation::HeaderPowData(bytes) => {
            p2p_header.pow.as_mut().unwrap().pow_data = bytes.clone();
            grpc_header.pow.as_mut().unwrap().pow_data = bytes.clone();
        },
        _ => {},
    }
    let direct = match (
        direct_header(&block.header, mutation),
        direct_body(&block.body, mutation),
    ) {
        (Some(header), _) => Some(Block::new(header, block.body.clone())),
        (None, Some(body)) => Some(Block::new(block.header.clone(), body)),
        (None, None) => None,
    };
    let (borsh_encoding, bincode_encoding) = binary_encodings(
        Some(borsh::to_vec(&block).unwrap()),
        bincode::serialize(&block).unwrap(),
        binary_mutation(&block.body, Some(&block.header), None, mutation),
        Some((
            Some(borsh::to_vec(&alternative).unwrap()),
            bincode::serialize(&alternative).unwrap(),
        )),
        direct.map(|b| (Some(borsh::to_vec(&b).unwrap()), bincode::serialize(&b).unwrap())),
        mutation,
    );

    let mut results = vec![
        ("serde_json", serde_json::from_value::<Block>(json).ok()),
        ("P2P proto", Block::try_from(p2p).ok()),
        ("gRPC proto", Block::try_from(grpc_block).ok()),
    ];
    if let Some(encoding) = borsh_encoding {
        results.push(("borsh", borsh::from_slice::<Block>(&encoding).ok()));
    }
    if let Some(encoding) = bincode_encoding {
        results.push(("bincode", bincode::deserialize::<Block>(&encoding).ok()));
    }
    if let Mutation::Nothing = mutation {
        results.push(("original", Some(block.clone())));
    }
    // `PartialEq` of an input only compares the output it spends, so compare the full encodings instead
    let results = results
        .into_iter()
        .map(|(name, block)| (name, block.map(|b| borsh::to_vec(&b).unwrap())))
        .collect::<Vec<_>>();
    assert_parity(&sample, *accept, &results);
}

fn check_transaction(base: &Transaction, sample: &Sample) {
    let Sample { name, mutation, accept } = sample;
    let mutation = &mutation;
    let sample = format!("transaction {name}");
    let mut transaction = base.clone();
    transaction.body = prepared_body(&base.body, mutation);

    let mut json = serde_json::to_value(&transaction).unwrap();
    mutate_json(&mut json, mutation);
    let mut p2p = proto::types::Transaction::try_from(transaction.clone()).unwrap();
    mutate_p2p_body(p2p.body.as_mut().unwrap(), mutation);
    let mut grpc_transaction = grpc::Transaction::try_from(transaction.clone()).unwrap();
    mutate_grpc_body(grpc_transaction.body.as_mut().unwrap(), mutation);
    if let Mutation::TransactionOffset(bytes) = mutation {
        p2p.offset.as_mut().unwrap().data = bytes.clone();
        grpc_transaction.offset = bytes.clone();
    }
    let mut alternative = transaction.clone();
    alternative.body = alternative_body(&transaction.body, mutation);
    // `Transaction` has no borsh encoding
    let (_, bincode_encoding) = binary_encodings(
        None,
        bincode::serialize(&transaction).unwrap(),
        binary_mutation(&transaction.body, None, Some(&transaction.offset), mutation),
        Some((None, bincode::serialize(&alternative).unwrap())),
        direct_body(&transaction.body, mutation).map(|body| {
            let mut direct = transaction.clone();
            direct.body = body;
            (None, bincode::serialize(&direct).unwrap())
        }),
        mutation,
    );

    let mut results = vec![
        ("serde_json", serde_json::from_value::<Transaction>(json).ok()),
        ("P2P proto", Transaction::try_from(p2p).ok()),
        ("gRPC proto", Transaction::try_from(grpc_transaction).ok()),
    ];
    if let Some(encoding) = bincode_encoding {
        results.push(("bincode", bincode::deserialize::<Transaction>(&encoding).ok()));
    }
    if let Mutation::Nothing = mutation {
        results.push(("original", Some(transaction.clone())));
    }
    // `PartialEq` of an input only compares the output it spends, so compare the full encodings instead
    let results = results
        .into_iter()
        .map(|(name, transaction)| (name, transaction.map(|t| bincode::serialize(&t).unwrap())))
        .collect::<Vec<_>>();
    assert_parity(&sample, *accept, &results);
}

/// A change to a block or transaction, and whether every decoder must accept it
struct Sample {
    name: String,
    mutation: Mutation,
    accept: bool,
}

fn sample(name: impl Into<String>, mutation: Mutation, accept: bool) -> Sample {
    Sample {
        name: name.into(),
        mutation,
        accept,
    }
}

/// The bound of `PowData`
const MAX_POW_DATA_SIZE: usize = u16::MAX as usize;

/// A non-canonical 32 byte value: neither a valid compressed point nor a canonical scalar
const NON_CANONICAL: [u8; 32] = [0xff; 32];

/// What a 32 byte field holds, which decides whether a non-canonical value is accepted
#[derive(Clone, Copy)]
enum Fixed {
    /// A compressed point (key or commitment): its decoders only check the size, the point is decompressed when used
    Point,
    /// A scalar: its decoders reject a non-canonical encoding
    Scalar,
    /// A hash: any 32 bytes are valid
    Hash,
}

/// Samples for a 32 byte key, commitment, scalar or hash field: a valid value, a non-canonical value and the wrong
/// sizes
fn fixed_size_samples(name: &str, valid: Vec<u8>, kind: Fixed, mutation: fn(Vec<u8>) -> Mutation) -> Vec<Sample> {
    vec![
        sample(format!("{name}: a valid value"), mutation(valid), true),
        sample(
            format!("{name}: non-canonical"),
            mutation(NON_CANONICAL.to_vec()),
            !matches!(kind, Fixed::Scalar),
        ),
        sample(format!("{name}: 31 bytes"), mutation(vec![1; 31]), false),
        sample(format!("{name}: 33 bytes"), mutation(vec![1; 33]), false),
        sample(format!("{name}: empty"), mutation(vec![]), false),
    ]
}

/// Samples of a version byte: `valid` versions are accepted, others (and values over a byte) rejected
fn version_samples(name: &str, valid: &[u32], mutation: fn(u32) -> Mutation) -> Vec<Sample> {
    [0, 1, 2, 0xff, 0x100]
        .into_iter()
        .map(|v| sample(format!("{name} {v:#x}"), mutation(v), valid.contains(&v)))
        .collect()
}

#[allow(clippy::too_many_lines)]
fn side_chain_samples() -> Vec<Sample> {
    let signature = || CompressedSignature::new(random_public_key(), PrivateKey::random(&mut rand::rng()));
    let sidechain_id = Some(SideChainId::new(random_public_key(), signature()));
    let registration = SideChainFeature {
        data: SideChainFeatureData::ValidatorNodeRegistration(Box::new(ValidatorNodeRegistration::new(
            ValidatorNodeSignature::new(random_public_key(), signature()),
            random_public_key(),
            VnEpoch(10),
        ))),
        sidechain_id: sidechain_id.clone(),
    };
    let exit = SideChainFeature {
        data: SideChainFeatureData::ValidatorNodeExit(ValidatorNodeExit::new(
            ValidatorNodeSignature::new(random_public_key(), signature()),
            VnEpoch(10),
        )),
        sidechain_id: None,
    };
    let template = SideChainFeature {
        data: SideChainFeatureData::CodeTemplateRegistration(CodeTemplateRegistration {
            author_public_key: random_public_key(),
            author_signature: signature(),
            template_name: "decoder-parity-template".to_string().try_into().unwrap(),
            template_version: 1,
            template_type: TemplateType::Wasm { abi_version: 1 },
            build_info: BuildInfo {
                repo_url: "https://example.com".to_string().try_into().unwrap(),
                commit_hash: vec![0x11u8; 20].try_into().unwrap(),
            },
            binary_sha: FixedHash::from([2u8; 32]),
            binary_url: "https://example.com/decoder-parity-binary"
                .to_string()
                .try_into()
                .unwrap(),
        }),
        sidechain_id: None,
    };
    let confidential = SideChainFeature {
        data: SideChainFeatureData::ConfidentialOutput(ConfidentialOutputData {
            claim_public_key: random_public_key(),
        }),
        sidechain_id: None,
    };

    let mut changes = Vec::new();
    for (name, feature) in [
        ("validator node registration", &registration),
        ("validator node exit", &exit),
        ("template registration", &template),
        ("confidential output", &confidential),
    ] {
        changes.push((format!("{name}: valid"), feature, SideChainChange::Nothing, true));
        changes.push((
            format!("{name}: another valid key"),
            feature,
            SideChainChange::DataPublicKey(random_public_key_bytes()),
            true,
        ));
        // Compressed keys are only decompressed when used
        changes.push((
            format!("{name}: non-canonical key"),
            feature,
            SideChainChange::DataPublicKey(NON_CANONICAL.to_vec()),
            true,
        ));
        changes.push((
            format!("{name}: 31 byte key"),
            feature,
            SideChainChange::DataPublicKey(vec![1; 31]),
            false,
        ));
    }
    let registration_changes = [
        (
            "claim key valid",
            SideChainChange::RegistrationClaimPublicKey(random_public_key_bytes()),
            true,
        ),
        (
            "claim key non-canonical",
            SideChainChange::RegistrationClaimPublicKey(NON_CANONICAL.to_vec()),
            true,
        ),
        (
            "claim key 31 bytes",
            SideChainChange::RegistrationClaimPublicKey(vec![1; 31]),
            false,
        ),
        (
            "sidechain id key valid",
            SideChainChange::SidechainIdPublicKey(random_public_key_bytes()),
            true,
        ),
        (
            "sidechain id key non-canonical",
            SideChainChange::SidechainIdPublicKey(NON_CANONICAL.to_vec()),
            true,
        ),
        (
            "sidechain id key 31 bytes",
            SideChainChange::SidechainIdPublicKey(vec![1; 31]),
            false,
        ),
        (
            "sidechain id signature valid",
            SideChainChange::SidechainIdSignature(random_scalar_bytes()),
            true,
        ),
        (
            "sidechain id signature non-canonical",
            SideChainChange::SidechainIdSignature(NON_CANONICAL.to_vec()),
            false,
        ),
        (
            "sidechain id signature 31 bytes",
            SideChainChange::SidechainIdSignature(vec![1; 31]),
            false,
        ),
    ];
    for (name, change, accept) in registration_changes {
        changes.push((
            format!("validator node registration: {name}"),
            &registration,
            change,
            accept,
        ));
    }
    let template_changes = [
        ("name of 32 bytes", SideChainChange::TemplateName("n".repeat(32)), true),
        ("name of 33 bytes", SideChainChange::TemplateName("n".repeat(33)), false),
        (
            "version u16::MAX",
            SideChainChange::TemplateVersion(u32::from(u16::MAX)),
            true,
        ),
        (
            "version u16::MAX + 1",
            SideChainChange::TemplateVersion(u32::from(u16::MAX) + 1),
            false,
        ),
        (
            "binary sha of 32 bytes",
            SideChainChange::TemplateBinarySha(vec![3; 32]),
            true,
        ),
        (
            "binary sha of 31 bytes",
            SideChainChange::TemplateBinarySha(vec![3; 31]),
            false,
        ),
        (
            "binary sha of 33 bytes",
            SideChainChange::TemplateBinarySha(vec![3; 33]),
            false,
        ),
        (
            "binary url of 255 bytes",
            SideChainChange::TemplateBinaryUrl("u".repeat(255)),
            true,
        ),
        (
            "binary url of 256 bytes",
            SideChainChange::TemplateBinaryUrl("u".repeat(256)),
            false,
        ),
        (
            "commit hash of 32 bytes",
            SideChainChange::TemplateCommitHash(vec![4; 32]),
            true,
        ),
        (
            "commit hash of 33 bytes",
            SideChainChange::TemplateCommitHash(vec![4; 33]),
            false,
        ),
        (
            "wasm template, abi version 1",
            SideChainChange::TemplateType(TemplateTypeSample::Wasm(1)),
            true,
        ),
        (
            "wasm template, abi version u16::MAX",
            SideChainChange::TemplateType(TemplateTypeSample::Wasm(u32::from(u16::MAX))),
            true,
        ),
        (
            "wasm template, abi version u16::MAX + 1",
            SideChainChange::TemplateType(TemplateTypeSample::Wasm(u32::from(u16::MAX) + 1)),
            false,
        ),
        (
            "flow template",
            SideChainChange::TemplateType(TemplateTypeSample::Flow),
            true,
        ),
        (
            "manifest template",
            SideChainChange::TemplateType(TemplateTypeSample::Manifest),
            true,
        ),
    ];
    for (name, change, accept) in template_changes {
        changes.push((format!("template registration: {name}"), &template, change, accept));
    }
    changes
        .into_iter()
        .map(|(name, feature, change, accept)| {
            sample(
                format!("side-chain {name}"),
                Mutation::OutputSideChain(Box::new(feature.clone()), change),
                accept,
            )
        })
        .collect()
}

/// Body samples, shared by blocks and transactions. Each is named after the change it makes.
#[allow(clippy::too_many_lines)]
fn body_samples() -> Vec<Sample> {
    let key = random_public_key();
    let valid_script = script!(PushPubKey(Box::new(key.clone()))).unwrap().to_bytes();
    let valid_stack = ExecutionStack::new(vec![StackItem::Number(7), StackItem::PublicKey(key)]).to_bytes();
    let mut samples = vec![sample("unchanged", Mutation::Nothing, true)];

    // EncryptedData: STATIC_ENCRYPTED_DATA_SIZE_TOTAL..=MAX_ENCRYPTED_DATA_SIZE bytes
    let encrypted_data_size_is_valid =
        |len: usize| (STATIC_ENCRYPTED_DATA_SIZE_TOTAL..=MAX_ENCRYPTED_DATA_SIZE).contains(&len);
    for len in [
        0,
        1,
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1,
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL + 1,
        MAX_ENCRYPTED_DATA_SIZE,
        MAX_ENCRYPTED_DATA_SIZE + 1,
    ] {
        samples.push(sample(
            format!("encrypted data of {len} bytes"),
            Mutation::OutputEncryptedData(vec![0xab; len]),
            encrypted_data_size_is_valid(len),
        ));
    }

    // TariScript: well formed opcodes, at most MAX_SCRIPT_BYTES
    samples.push(sample(
        "a valid script",
        Mutation::OutputScript(valid_script.clone()),
        true,
    ));
    samples.push(sample("an empty script", Mutation::OutputScript(vec![]), true));
    samples.push(sample(
        "a truncated script",
        Mutation::OutputScript(valid_script[..valid_script.len() - 1].to_vec()),
        false,
    ));
    samples.push(sample("an unknown opcode", Mutation::OutputScript(vec![0xff]), false));
    samples.push(sample(
        "a script over MAX_SCRIPT_BYTES",
        Mutation::OutputScript(valid_script.repeat(MAX_SCRIPT_BYTES / valid_script.len() + 1)),
        false,
    ));

    // Covenant: well formed tokens, no trailing bytes, at most MAX_COVENANT_BYTES
    let hash = FixedHash::from([3u8; 32]);
    let valid_covenant = covenant!(output_hash_eq(@hash(hash))).unwrap().to_bytes();
    samples.push(sample(
        "a valid covenant",
        Mutation::OutputCovenant(valid_covenant.clone()),
        true,
    ));
    samples.push(sample("an empty covenant", Mutation::OutputCovenant(vec![]), true));
    samples.push(sample(
        "a covenant with trailing bytes",
        Mutation::OutputCovenant([valid_covenant.clone(), vec![0]].concat()),
        false,
    ));
    samples.push(sample(
        "an unknown covenant token",
        Mutation::OutputCovenant(vec![0xff]),
        false,
    ));
    samples.push(sample(
        "a covenant over MAX_COVENANT_BYTES",
        Mutation::OutputCovenant(valid_covenant.repeat(4096 / valid_covenant.len() + 1)),
        false,
    ));

    // Versions
    samples.extend(version_samples("output version", &[0, 1], Mutation::OutputVersion));
    samples.extend(version_samples("full input version", &[0, 1], Mutation::InputVersion));
    samples.extend(version_samples("kernel version", &[0], Mutation::KernelVersion));

    // Output features: known version, output type and range proof type; coinbase extra of at most 258 bytes
    samples.extend(version_samples(
        "output features version",
        &[0, 1],
        Mutation::OutputFeaturesVersion,
    ));
    for (output_type, accept) in [
        (0, true),
        (1, true),
        (7, true),
        (8, false),
        (0xff, false),
        (0x100, false),
    ] {
        samples.push(sample(
            format!("output type {output_type:#x}"),
            Mutation::OutputType(output_type),
            accept,
        ));
    }
    for (range_proof_type, accept) in [(0, true), (1, true), (2, false), (0xff, false), (0x100, false)] {
        samples.push(sample(
            format!("range proof type {range_proof_type:#x}"),
            Mutation::OutputRangeProofType(range_proof_type),
            accept,
        ));
    }
    for len in [0, 1, 258, 259] {
        samples.push(sample(
            format!("coinbase extra of {len} bytes"),
            Mutation::OutputCoinbaseExtra(vec![5; len]),
            len <= 258,
        ));
    }

    // Range proof bytes: not validated when decoded (the proof is verified by consensus validation)
    for (name, bytes) in [("empty", vec![]), ("1 byte", vec![1]), ("garbage", vec![0xff; 100])] {
        samples.push(sample(
            format!("range proof: {name}"),
            Mutation::OutputRangeProof(bytes),
            true,
        ));
    }

    // Keys, commitments, signature scalars and hashes
    samples.extend(fixed_size_samples(
        "output commitment",
        random_public_key_bytes(),
        Fixed::Point,
        Mutation::OutputCommitment,
    ));
    samples.extend(fixed_size_samples(
        "sender offset public key",
        random_public_key_bytes(),
        Fixed::Point,
        Mutation::OutputSenderOffsetPublicKey,
    ));
    samples.extend(fixed_size_samples(
        "metadata signature u_a",
        random_scalar_bytes(),
        Fixed::Scalar,
        Mutation::OutputMetadataSignatureUa,
    ));
    samples.extend(fixed_size_samples(
        "input script signature ephemeral pubkey",
        random_public_key_bytes(),
        Fixed::Point,
        Mutation::InputScriptSignatureEphemeralPubkey,
    ));
    samples.extend(fixed_size_samples(
        "kernel excess",
        random_public_key_bytes(),
        Fixed::Point,
        Mutation::KernelExcess,
    ));
    samples.extend(fixed_size_samples(
        "kernel excess signature",
        random_scalar_bytes(),
        Fixed::Scalar,
        Mutation::KernelExcessSignature,
    ));
    // An empty burn commitment means "none" in gRPC but is invalid in P2P, so only present values are sampled
    for (name, bytes, accept) in [
        ("a valid value", random_public_key_bytes(), true),
        ("non-canonical", NON_CANONICAL.to_vec(), true),
        ("31 bytes", vec![1; 31], false),
        ("33 bytes", vec![1; 33], false),
    ] {
        samples.push(sample(
            format!("kernel burn commitment: {name}"),
            Mutation::KernelBurnCommitment(bytes),
            accept,
        ));
    }

    // Side-chain features
    samples.extend(side_chain_samples());

    // The full (non-compact) input
    samples.push(sample(
        "full input: a valid script",
        Mutation::FullInputScript(valid_script.clone()),
        true,
    ));
    samples.push(sample(
        "full input: an unknown opcode",
        Mutation::FullInputScript(vec![0xff]),
        false,
    ));
    for len in [
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1,
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
        MAX_ENCRYPTED_DATA_SIZE + 1,
    ] {
        samples.push(sample(
            format!("full input: encrypted data of {len} bytes"),
            Mutation::FullInputEncryptedData(vec![0xab; len]),
            encrypted_data_size_is_valid(len),
        ));
    }
    samples.extend(fixed_size_samples(
        "full input commitment",
        random_public_key_bytes(),
        Fixed::Point,
        Mutation::FullInputCommitment,
    ));
    samples.extend(fixed_size_samples(
        "full input sender offset public key",
        random_public_key_bytes(),
        Fixed::Point,
        Mutation::FullInputSenderOffsetPublicKey,
    ));
    samples.extend(fixed_size_samples(
        "full input metadata signature u_a",
        random_scalar_bytes(),
        Fixed::Scalar,
        Mutation::FullInputMetadataSignatureUa,
    ));
    samples.extend(fixed_size_samples(
        "full input rangeproof hash",
        vec![9; 32],
        Fixed::Hash,
        Mutation::FullInputRangeproofHash,
    ));
    for (output_type, accept) in [(0, true), (7, true), (8, false), (0x100, false)] {
        samples.push(sample(
            format!("full input output type {output_type:#x}"),
            Mutation::FullInputOutputType(output_type),
            accept,
        ));
    }

    // ExecutionStack: well formed items, at most MAX_STACK_SIZE of them
    samples.push(sample(
        "valid input data",
        Mutation::InputData(valid_stack.clone()),
        true,
    ));
    samples.push(sample("empty input data", Mutation::InputData(vec![]), true));
    samples.push(sample(
        "truncated input data",
        Mutation::InputData(valid_stack[..valid_stack.len() - 1].to_vec()),
        false,
    ));
    samples.push(sample(
        "an unknown stack item type",
        Mutation::InputData(vec![0xff]),
        false,
    ));
    let number = ExecutionStack::new(vec![StackItem::Number(1)]).to_bytes();
    samples.push(sample(
        "input data over MAX_STACK_SIZE items",
        Mutation::InputData(number.repeat(256)),
        false,
    ));

    // KernelFeatures: known bits only, a single byte
    for bits in [0, 1, 2, 3, 4, 0x80, 0xff, 0x100] {
        samples.push(sample(
            format!("kernel features {bits:#x}"),
            Mutation::KernelFeatures(bits),
            bits <= 3,
        ));
    }
    samples
}

#[test]
fn block_and_transaction_decoders_accept_and_reject_the_same_samples() {
    let block = base_block();
    let transaction = transaction_with_body(block.body.clone());
    for sample in body_samples() {
        check_block(&block, &sample);
        check_transaction(&transaction, &sample);
    }
}

#[test]
fn block_header_decoders_accept_and_reject_the_same_samples() {
    let block = base_block();
    let mut samples = Vec::new();
    for len in [0, 31, 32, 33] {
        samples.push(sample(
            format!("block_output_mr of {len} bytes"),
            Mutation::HeaderBlockOutputMr(vec![7; len]),
            len == 32,
        ));
    }
    for field in [
        HeaderHash::PrevHash,
        HeaderHash::OutputMr,
        HeaderHash::KernelMr,
        HeaderHash::InputMr,
        HeaderHash::ValidatorNodeMr,
    ] {
        for len in [0, 31, 32, 33] {
            samples.push(sample(
                format!("{field:?} of {len} bytes"),
                Mutation::HeaderHash(field, vec![7; len]),
                len == 32,
            ));
        }
    }
    for version in [0, 1, u32::from(u16::MAX), u32::from(u16::MAX) + 1] {
        samples.push(sample(
            format!("header version {version:#x}"),
            Mutation::HeaderVersion(version),
            version <= u32::from(u16::MAX),
        ));
    }
    samples.extend(fixed_size_samples(
        "total kernel offset",
        random_scalar_bytes(),
        Fixed::Scalar,
        Mutation::HeaderTotalKernelOffset,
    ));
    for algo in [0, 1, 2, 3, 4, 0xff, 0x100] {
        samples.push(sample(
            format!("pow algorithm {algo:#x}"),
            Mutation::HeaderPowAlgo(algo),
            algo <= 3,
        ));
    }
    for len in [0, 1, MAX_POW_DATA_SIZE, MAX_POW_DATA_SIZE + 1] {
        samples.push(sample(
            format!("pow data of {len} bytes"),
            Mutation::HeaderPowData(vec![6; len]),
            len <= MAX_POW_DATA_SIZE,
        ));
    }
    for sample in samples {
        check_block(&block, &sample);
    }
}

#[test]
fn transaction_offset_decoders_accept_and_reject_the_same_samples() {
    let transaction = transaction_with_body(base_body());
    for sample in fixed_size_samples(
        "transaction offset",
        random_scalar_bytes(),
        Fixed::Scalar,
        Mutation::TransactionOffset,
    ) {
        check_transaction(&transaction, &sample);
    }
}

/// Neither protobuf family carries the version of a compact input, so both decode a compact V1 input as V0, while
/// serde, bincode and borsh keep V1. The node's P2P round-trip relies on this to normalise the version; if a protobuf
/// family starts carrying it, this fails and the compact input version should be sampled like the others.
#[test]
fn the_protobuf_families_decode_a_compact_v1_input_as_v0() {
    let (mut inputs, outputs, kernels) = base_body().dissolve();
    let compact = inputs.first_mut().unwrap();
    assert!(compact.is_compact());
    compact.version = TransactionInputVersion::V1;
    let transaction = transaction_with_body(AggregateBody::new_unsorted(inputs, outputs, kernels));
    let version = |transaction: &Transaction| transaction.body.inputs().first().unwrap().version;

    let p2p = Transaction::try_from(proto::types::Transaction::try_from(transaction.clone()).unwrap()).unwrap();
    assert_eq!(version(&p2p), TransactionInputVersion::V0);
    let grpc = Transaction::try_from(grpc::Transaction::try_from(transaction.clone()).unwrap()).unwrap();
    assert_eq!(version(&grpc), TransactionInputVersion::V0);

    let json = serde_json::from_value::<Transaction>(serde_json::to_value(&transaction).unwrap()).unwrap();
    assert_eq!(version(&json), TransactionInputVersion::V1);
    let bincode = bincode::deserialize::<Transaction>(&bincode::serialize(&transaction).unwrap()).unwrap();
    assert_eq!(version(&bincode), TransactionInputVersion::V1);
    let body = AggregateBody::try_from_slice(&borsh::to_vec(&transaction.body).unwrap()).unwrap();
    assert_eq!(body.inputs().first().unwrap().version, TransactionInputVersion::V1);
}

/// Compiles only if the serde and borsh decoders of `T` are the ones `impl_validated_decode!` generates: the macro is
/// the only implementor of `DecodesViaValidatedDecode`
fn assert_validated<T: ValidatedDecode + DecodesViaValidatedDecode>() {}

/// Every type with an invariant that its serde and borsh decoders must enforce. Removing a type's
/// `impl_validated_decode!` (for example to derive its decoders again) fails to compile here, whatever
/// `scripts/decoder_parity_check.py` can see.
#[test]
fn every_invariant_type_decodes_through_validated_decode() {
    assert_validated::<EncryptedData>();
    assert_validated::<KernelFeatures>();
    assert_validated::<ExecutionStack>();
    assert_validated::<TariScript>();
    assert_validated::<Covenant>();
    assert_validated::<MemoField>();
    assert_validated::<Difficulty>();
    assert_validated::<TariKeyId>();
}

#[test]
fn encrypted_data_size_bounds_are_the_ones_the_samples_use() {
    // Keeps the samples meaningful if the bounds change
    assert!(EncryptedData::from_bytes(&[0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1]).is_err());
    assert!(EncryptedData::from_bytes(&[0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL]).is_ok());
    assert!(EncryptedData::from_bytes(&[0; MAX_ENCRYPTED_DATA_SIZE]).is_ok());
    assert!(EncryptedData::from_bytes(&[0; MAX_ENCRYPTED_DATA_SIZE + 1]).is_err());
}
