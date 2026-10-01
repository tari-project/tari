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

use minotari_app_grpc::tari_rpc as grpc;
use rand::RngExt;
use serde_json::Value;
use tari_common::configuration::Network;
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, CompressedSignature, FixedHash, PrivateKey},
};
use tari_core::{blocks::genesis_block::get_genesis_block, proto};
use tari_crypto::keys::SecretKey;
use tari_node_components::blocks::Block;
use tari_script::{ExecutionStack, MAX_SCRIPT_BYTES, StackItem, script};
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    covenant,
    tari_proof_of_work::PowAlgorithm,
    transaction_components::{
        BuildInfo,
        CodeTemplateRegistration,
        ConfidentialOutputData,
        EncryptedData,
        OutputFeaturesVersion,
        RangeProofType,
        SideChainFeature,
        SideChainFeatureData,
        SideChainId,
        TemplateType,
        Transaction,
        TransactionInput,
        ValidatorNodeExit,
        ValidatorNodeRegistration,
        ValidatorNodeSignature,
        encrypted_data::{MAX_ENCRYPTED_DATA_SIZE, STATIC_ENCRYPTED_DATA_SIZE_TOTAL},
    },
};
use tari_utilities::{ByteArray, hex::to_hex};

/// Asserts that every decoder made the same decision on `sample`: either all rejected it, or all accepted it and
/// decoded it to the same value. `results` holds the name of each decoder and its result (`None` if it rejected the
/// sample).
fn assert_parity<T: PartialEq + Debug>(sample: &str, results: &[(&str, Option<T>)]) {
    let verdict = |result: &Option<T>| if result.is_some() { "accepted" } else { "rejected" };
    let (first_name, first) = results.first().expect("at least one decoder");
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
    /// Installs a (valid) side-chain feature on the first output, then changes one of its fields
    OutputSideChain(Box<SideChainFeature>, SideChainChange),
    // The first input
    InputData(Vec<u8>),
    InputScriptSignatureEphemeralPubkey(Vec<u8>),
    // The first kernel
    KernelFeatures(u32),
    KernelExcess(Vec<u8>),
    KernelExcessSignature(Vec<u8>),
    // Block only
    HeaderBlockOutputMr(Vec<u8>),
    HeaderVersion(u32),
    HeaderTotalKernelOffset(Vec<u8>),
    HeaderPowAlgo(u64),
    HeaderPowData(Vec<u8>),
    // Transaction only
    TransactionOffset(Vec<u8>),
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
    let output = outputs.first_mut().unwrap();
    output.script = script!(PushPubKey(Box::new(random_public_key()))).unwrap();
    let hash = FixedHash::from(rand::rng().random::<[u8; 32]>());
    output.covenant = covenant!(output_hash_eq(@hash(hash))).unwrap();
    AggregateBody::new_unsorted(vec![input], outputs, kernels)
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

/// The base body, with the side-chain feature of an `OutputSideChain` mutation installed on the first output
fn prepared_body(body: &AggregateBody, mutation: &Mutation) -> AggregateBody {
    let Mutation::OutputSideChain(feature, _) = mutation else {
        return body.clone();
    };
    let (inputs, mut outputs, kernels) = body.clone().dissolve();
    outputs[0].features.sidechain_feature = Some(*feature.clone());
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
    let input = &mut body.inputs[0];
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
        Mutation::KernelFeatures(bits) => kernel.features = *bits,
        Mutation::KernelExcess(bytes) => kernel.excess.as_mut().unwrap().data = bytes.clone(),
        Mutation::KernelExcessSignature(bytes) => kernel.excess_sig.as_mut().unwrap().signature = bytes.clone(),
        _ => {},
    }
}

fn mutate_grpc_body(body: &mut grpc::AggregateBody, mutation: &Mutation) {
    let output = &mut body.outputs[0];
    let input = &mut body.inputs[0];
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
        Mutation::KernelFeatures(bits) => kernel.features = *bits,
        Mutation::KernelExcess(bytes) => kernel.excess = bytes.clone(),
        Mutation::KernelExcessSignature(bytes) => kernel.excess_sig.as_mut().unwrap().signature = bytes.clone(),
        _ => {},
    }
}

/// The encodings of a changed field before and after the change: `(borsh_before, borsh_after, bincode_before,
/// bincode_after)`
type BinaryMutation = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

/// A same-size change of a fixed size field (a key, commitment or scalar), which appears as its raw bytes in both
/// binary formats. `None` if the new value has a different size, which the binary formats can not express.
fn same_size(before: &[u8], after: &[u8]) -> Option<BinaryMutation> {
    if before.len() != after.len() {
        return None;
    }
    Some((before.to_vec(), after.to_vec(), before.to_vec(), after.to_vec()))
}

/// The [`BinaryMutation`] of `mutation`. `None` if the binary formats are not checked for it: they can not express
/// the change (a fixed size field of the wrong size, a value that does not fit the field's type), or the field is
/// not unique enough in the encoding to splice.
fn binary_mutation(body: &AggregateBody, mutation: &Mutation) -> Option<BinaryMutation> {
    let output = body.outputs().first().unwrap();
    let input = body.inputs().first().unwrap();
    let kernel = body.kernels().first().unwrap();
    match mutation {
        Mutation::Nothing => Some(Default::default()),
        Mutation::OutputEncryptedData(bytes) => {
            // borsh: a `u32` length prefix; bincode: a `u64` length prefix, the same as a `Vec<u8>`
            let before = output.encrypted_data.as_bytes().to_vec();
            assert_eq!(
                borsh::to_vec(&output.encrypted_data).unwrap(),
                borsh::to_vec(&before).unwrap()
            );
            assert_eq!(
                bincode::serialize(&output.encrypted_data).unwrap(),
                bincode::serialize(&before).unwrap()
            );
            Some((
                borsh::to_vec(&before).unwrap(),
                borsh::to_vec(bytes).unwrap(),
                bincode::serialize(&before).unwrap(),
                bincode::serialize(bytes).unwrap(),
            ))
        },
        Mutation::OutputScript(bytes) | Mutation::OutputCovenant(bytes) | Mutation::InputData(bytes) => {
            // borsh: a varint length prefix; bincode: a `u64` length prefix, the same as a `Vec<u8>`
            let (borsh_before, bincode_before) = match mutation {
                Mutation::OutputScript(_) => (
                    borsh::to_vec(&output.script).unwrap(),
                    bincode::serialize(&output.script).unwrap(),
                ),
                Mutation::OutputCovenant(_) => (
                    borsh::to_vec(&output.covenant).unwrap(),
                    bincode::serialize(&output.covenant).unwrap(),
                ),
                _ => (
                    borsh::to_vec(&input.input_data).unwrap(),
                    bincode::serialize(&input.input_data).unwrap(),
                ),
            };
            Some((
                borsh_before,
                varint_prefixed(bytes),
                bincode_before,
                bincode::serialize(bytes).unwrap(),
            ))
        },
        Mutation::KernelFeatures(bits) => {
            let bits = u8::try_from(*bits).ok()?;
            // In both formats the features byte directly follows the version
            let borsh_before = borsh::to_vec(kernel).unwrap();
            let mut borsh_after = borsh_before.clone();
            borsh_after[borsh::to_vec(&kernel.version).unwrap().len()] = bits;
            let bincode_before = bincode::serialize(kernel).unwrap();
            let mut bincode_after = bincode_before.clone();
            bincode_after[bincode::serialize(&kernel.version).unwrap().len()] = bits;
            Some((borsh_before, borsh_after, bincode_before, bincode_after))
        },
        Mutation::OutputCommitment(bytes) => same_size(output.commitment.as_bytes(), bytes),
        Mutation::OutputSenderOffsetPublicKey(bytes) => same_size(output.sender_offset_public_key.as_bytes(), bytes),
        Mutation::OutputMetadataSignatureUa(bytes) => same_size(output.metadata_signature.u_a().as_bytes(), bytes),
        Mutation::KernelExcess(bytes) => same_size(kernel.excess.as_bytes(), bytes),
        Mutation::KernelExcessSignature(bytes) => same_size(kernel.excess_sig.get_signature().as_bytes(), bytes),
        _ => None,
    }
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

fn check_block(base: &Block, name: &str, mutation: &Mutation) {
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
    let mut binary = binary_mutation(&block.body, mutation);
    match mutation {
        Mutation::HeaderBlockOutputMr(bytes) => {
            p2p_header.block_output_mr = bytes.clone();
            grpc_header.block_output_mr = bytes.clone();
        },
        Mutation::HeaderVersion(version) => {
            p2p_header.version = *version;
            grpc_header.version = *version;
        },
        Mutation::HeaderTotalKernelOffset(bytes) => {
            p2p_header.total_kernel_offset = bytes.clone();
            grpc_header.total_kernel_offset = bytes.clone();
            binary = same_size(block.header.total_kernel_offset.as_bytes(), bytes);
        },
        Mutation::HeaderPowAlgo(algo) => {
            p2p_header.pow.as_mut().unwrap().pow_algo = *algo;
            grpc_header.pow.as_mut().unwrap().pow_algo = *algo;
        },
        Mutation::HeaderPowData(bytes) => {
            p2p_header.pow.as_mut().unwrap().pow_data = bytes.clone();
            grpc_header.pow.as_mut().unwrap().pow_data = bytes.clone();
        },
        _ => {},
    }

    let mut results = vec![
        ("serde_json", serde_json::from_value::<Block>(json).ok()),
        ("P2P proto", Block::try_from(p2p).ok()),
        ("gRPC proto", Block::try_from(grpc_block).ok()),
    ];
    if let Some((borsh_before, borsh_after, bincode_before, bincode_after)) = binary {
        let borsh_encoding = splice(&borsh::to_vec(&block).unwrap(), &borsh_before, &borsh_after);
        let bincode_encoding = splice(&bincode::serialize(&block).unwrap(), &bincode_before, &bincode_after);
        results.push(("borsh", borsh::from_slice::<Block>(&borsh_encoding).ok()));
        results.push(("bincode", bincode::deserialize::<Block>(&bincode_encoding).ok()));
    }
    if let Mutation::Nothing = mutation {
        results.push(("original", Some(block.clone())));
    }
    // `PartialEq` of an input only compares the output it spends, so compare the full encodings instead
    let results = results
        .into_iter()
        .map(|(name, block)| (name, block.map(|b| borsh::to_vec(&b).unwrap())))
        .collect::<Vec<_>>();
    assert_parity(&sample, &results);
}

fn check_transaction(base: &Transaction, name: &str, mutation: &Mutation) {
    let sample = format!("transaction {name}");
    let mut transaction = base.clone();
    transaction.body = prepared_body(&base.body, mutation);

    let mut json = serde_json::to_value(&transaction).unwrap();
    mutate_json(&mut json, mutation);
    let mut p2p = proto::types::Transaction::try_from(transaction.clone()).unwrap();
    mutate_p2p_body(p2p.body.as_mut().unwrap(), mutation);
    let mut grpc_transaction = grpc::Transaction::try_from(transaction.clone()).unwrap();
    mutate_grpc_body(grpc_transaction.body.as_mut().unwrap(), mutation);
    let mut binary = binary_mutation(&transaction.body, mutation);
    if let Mutation::TransactionOffset(bytes) = mutation {
        p2p.offset.as_mut().unwrap().data = bytes.clone();
        grpc_transaction.offset = bytes.clone();
        binary = same_size(transaction.offset.as_bytes(), bytes);
    }

    // `Transaction` has no borsh encoding
    let mut results = vec![
        ("serde_json", serde_json::from_value::<Transaction>(json).ok()),
        ("P2P proto", Transaction::try_from(p2p).ok()),
        ("gRPC proto", Transaction::try_from(grpc_transaction).ok()),
    ];
    if let Some((_, _, bincode_before, bincode_after)) = binary {
        let bincode_encoding = splice(
            &bincode::serialize(&transaction).unwrap(),
            &bincode_before,
            &bincode_after,
        );
        results.push(("bincode", bincode::deserialize::<Transaction>(&bincode_encoding).ok()));
    }
    if let Mutation::Nothing = mutation {
        results.push(("original", Some(transaction.clone())));
    }
    // `PartialEq` of an input only compares the output it spends, so compare the full encodings instead
    let results = results
        .into_iter()
        .map(|(name, transaction)| (name, transaction.map(|t| bincode::serialize(&t).unwrap())))
        .collect::<Vec<_>>();
    assert_parity(&sample, &results);
}

/// The bound of `PowData`
const MAX_POW_DATA_SIZE: usize = u16::MAX as usize;

/// A non-canonical 32 byte value: neither a valid compressed point nor a canonical scalar
const NON_CANONICAL: [u8; 32] = [0xff; 32];

/// Samples for a 32 byte key, commitment or scalar field: a valid value, a non-canonical value and the wrong sizes
fn fixed_size_samples(name: &str, valid: Vec<u8>, mutation: fn(Vec<u8>) -> Mutation) -> Vec<(String, Mutation)> {
    vec![
        (format!("{name}: a valid value"), mutation(valid)),
        (format!("{name}: non-canonical"), mutation(NON_CANONICAL.to_vec())),
        (format!("{name}: 31 bytes"), mutation(vec![1; 31])),
        (format!("{name}: 33 bytes"), mutation(vec![1; 33])),
        (format!("{name}: empty"), mutation(vec![])),
    ]
}

#[allow(clippy::too_many_lines)]
fn side_chain_samples() -> Vec<(String, Mutation)> {
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
            template_name: "name".to_string().try_into().unwrap(),
            template_version: 1,
            template_type: TemplateType::Wasm { abi_version: 1 },
            build_info: BuildInfo {
                repo_url: "https://example.com".to_string().try_into().unwrap(),
                commit_hash: vec![1u8; 20].try_into().unwrap(),
            },
            binary_sha: FixedHash::from([2u8; 32]),
            binary_url: "https://example.com/binary".to_string().try_into().unwrap(),
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
        changes.push((format!("{name}: valid"), feature, SideChainChange::Nothing));
        changes.push((
            format!("{name}: another valid key"),
            feature,
            SideChainChange::DataPublicKey(random_public_key_bytes()),
        ));
        changes.push((
            format!("{name}: non-canonical key"),
            feature,
            SideChainChange::DataPublicKey(NON_CANONICAL.to_vec()),
        ));
        changes.push((
            format!("{name}: 31 byte key"),
            feature,
            SideChainChange::DataPublicKey(vec![1; 31]),
        ));
    }
    let registration_changes = [
        (
            "claim key valid",
            SideChainChange::RegistrationClaimPublicKey(random_public_key_bytes()),
        ),
        (
            "claim key non-canonical",
            SideChainChange::RegistrationClaimPublicKey(NON_CANONICAL.to_vec()),
        ),
        (
            "claim key 31 bytes",
            SideChainChange::RegistrationClaimPublicKey(vec![1; 31]),
        ),
        (
            "sidechain id key valid",
            SideChainChange::SidechainIdPublicKey(random_public_key_bytes()),
        ),
        (
            "sidechain id key non-canonical",
            SideChainChange::SidechainIdPublicKey(NON_CANONICAL.to_vec()),
        ),
        (
            "sidechain id key 31 bytes",
            SideChainChange::SidechainIdPublicKey(vec![1; 31]),
        ),
        (
            "sidechain id signature valid",
            SideChainChange::SidechainIdSignature(random_scalar_bytes()),
        ),
        (
            "sidechain id signature non-canonical",
            SideChainChange::SidechainIdSignature(NON_CANONICAL.to_vec()),
        ),
        (
            "sidechain id signature 31 bytes",
            SideChainChange::SidechainIdSignature(vec![1; 31]),
        ),
    ];
    for (name, change) in registration_changes {
        changes.push((format!("validator node registration: {name}"), &registration, change));
    }
    let template_changes = [
        ("name of 32 bytes", SideChainChange::TemplateName("n".repeat(32))),
        ("name of 33 bytes", SideChainChange::TemplateName("n".repeat(33))),
        (
            "version u16::MAX",
            SideChainChange::TemplateVersion(u32::from(u16::MAX)),
        ),
        (
            "version u16::MAX + 1",
            SideChainChange::TemplateVersion(u32::from(u16::MAX) + 1),
        ),
        (
            "binary sha of 32 bytes",
            SideChainChange::TemplateBinarySha(vec![3; 32]),
        ),
        (
            "binary sha of 31 bytes",
            SideChainChange::TemplateBinarySha(vec![3; 31]),
        ),
        (
            "binary sha of 33 bytes",
            SideChainChange::TemplateBinarySha(vec![3; 33]),
        ),
        (
            "binary url of 255 bytes",
            SideChainChange::TemplateBinaryUrl("u".repeat(255)),
        ),
        (
            "binary url of 256 bytes",
            SideChainChange::TemplateBinaryUrl("u".repeat(256)),
        ),
        (
            "commit hash of 32 bytes",
            SideChainChange::TemplateCommitHash(vec![4; 32]),
        ),
        (
            "commit hash of 33 bytes",
            SideChainChange::TemplateCommitHash(vec![4; 33]),
        ),
    ];
    for (name, change) in template_changes {
        changes.push((format!("template registration: {name}"), &template, change));
    }
    changes
        .into_iter()
        .map(|(name, feature, change)| {
            (
                format!("side-chain {name}"),
                Mutation::OutputSideChain(Box::new(feature.clone()), change),
            )
        })
        .collect()
}

/// Body samples, shared by blocks and transactions. Each is named after the change it makes.
#[allow(clippy::too_many_lines)]
fn body_samples() -> Vec<(String, Mutation)> {
    let key = random_public_key();
    let valid_script = script!(PushPubKey(Box::new(key.clone()))).unwrap().to_bytes();
    let valid_stack = ExecutionStack::new(vec![StackItem::Number(7), StackItem::PublicKey(key)]).to_bytes();
    let mut samples = vec![("unchanged".to_string(), Mutation::Nothing)];

    // EncryptedData: STATIC_ENCRYPTED_DATA_SIZE_TOTAL..=MAX_ENCRYPTED_DATA_SIZE bytes
    for len in [
        0,
        1,
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1,
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
        STATIC_ENCRYPTED_DATA_SIZE_TOTAL + 1,
        MAX_ENCRYPTED_DATA_SIZE,
        MAX_ENCRYPTED_DATA_SIZE + 1,
    ] {
        samples.push((
            format!("encrypted data of {len} bytes"),
            Mutation::OutputEncryptedData(vec![0xab; len]),
        ));
    }

    // TariScript: well formed opcodes, at most MAX_SCRIPT_BYTES
    samples.push((
        "a valid script".to_string(),
        Mutation::OutputScript(valid_script.clone()),
    ));
    samples.push(("an empty script".to_string(), Mutation::OutputScript(vec![])));
    samples.push((
        "a truncated script".to_string(),
        Mutation::OutputScript(valid_script[..valid_script.len() - 1].to_vec()),
    ));
    samples.push(("an unknown opcode".to_string(), Mutation::OutputScript(vec![0xff])));
    samples.push((
        "a script over MAX_SCRIPT_BYTES".to_string(),
        Mutation::OutputScript(valid_script.repeat(MAX_SCRIPT_BYTES / valid_script.len() + 1)),
    ));

    // Covenant: well formed tokens, no trailing bytes, at most MAX_COVENANT_BYTES
    let hash = FixedHash::from([3u8; 32]);
    let valid_covenant = covenant!(output_hash_eq(@hash(hash))).unwrap().to_bytes();
    samples.push((
        "a valid covenant".to_string(),
        Mutation::OutputCovenant(valid_covenant.clone()),
    ));
    samples.push(("an empty covenant".to_string(), Mutation::OutputCovenant(vec![])));
    samples.push((
        "a covenant with trailing bytes".to_string(),
        Mutation::OutputCovenant([valid_covenant.clone(), vec![0]].concat()),
    ));
    samples.push((
        "an unknown covenant token".to_string(),
        Mutation::OutputCovenant(vec![0xff]),
    ));
    samples.push((
        "a covenant over MAX_COVENANT_BYTES".to_string(),
        Mutation::OutputCovenant(valid_covenant.repeat(4096 / valid_covenant.len() + 1)),
    ));

    // Output features: known version, output type and range proof type; coinbase extra of at most 258 bytes
    for version in [0, 1, 2, 0xff, 0x100] {
        samples.push((
            format!("output features version {version:#x}"),
            Mutation::OutputFeaturesVersion(version),
        ));
    }
    for output_type in [0, 1, 7, 8, 0xff, 0x100] {
        samples.push((
            format!("output type {output_type:#x}"),
            Mutation::OutputType(output_type),
        ));
    }
    for range_proof_type in [0, 1, 2, 0xff, 0x100] {
        samples.push((
            format!("range proof type {range_proof_type:#x}"),
            Mutation::OutputRangeProofType(range_proof_type),
        ));
    }
    for len in [0, 1, 258, 259] {
        samples.push((
            format!("coinbase extra of {len} bytes"),
            Mutation::OutputCoinbaseExtra(vec![5; len]),
        ));
    }

    // Range proof bytes
    for (name, bytes) in [("empty", vec![]), ("1 byte", vec![1]), ("garbage", vec![0xff; 100])] {
        samples.push((format!("range proof: {name}"), Mutation::OutputRangeProof(bytes)));
    }

    // Keys, commitments and signature scalars
    samples.extend(fixed_size_samples(
        "output commitment",
        random_public_key_bytes(),
        Mutation::OutputCommitment,
    ));
    samples.extend(fixed_size_samples(
        "sender offset public key",
        random_public_key_bytes(),
        Mutation::OutputSenderOffsetPublicKey,
    ));
    samples.extend(fixed_size_samples(
        "metadata signature u_a",
        random_scalar_bytes(),
        Mutation::OutputMetadataSignatureUa,
    ));
    samples.extend(fixed_size_samples(
        "input script signature ephemeral pubkey",
        random_public_key_bytes(),
        Mutation::InputScriptSignatureEphemeralPubkey,
    ));
    samples.extend(fixed_size_samples(
        "kernel excess",
        random_public_key_bytes(),
        Mutation::KernelExcess,
    ));
    samples.extend(fixed_size_samples(
        "kernel excess signature",
        random_scalar_bytes(),
        Mutation::KernelExcessSignature,
    ));

    // Side-chain features
    samples.extend(side_chain_samples());

    // ExecutionStack: well formed items, at most MAX_STACK_SIZE of them
    samples.push(("valid input data".to_string(), Mutation::InputData(valid_stack.clone())));
    samples.push(("empty input data".to_string(), Mutation::InputData(vec![])));
    samples.push((
        "truncated input data".to_string(),
        Mutation::InputData(valid_stack[..valid_stack.len() - 1].to_vec()),
    ));
    samples.push((
        "an unknown stack item type".to_string(),
        Mutation::InputData(vec![0xff]),
    ));
    let number = ExecutionStack::new(vec![StackItem::Number(1)]).to_bytes();
    samples.push((
        "input data over MAX_STACK_SIZE items".to_string(),
        Mutation::InputData(number.repeat(256)),
    ));

    // KernelFeatures: known bits only, a single byte
    for bits in [0, 1, 2, 3, 4, 0x80, 0xff, 0x100] {
        samples.push((format!("kernel features {bits:#x}"), Mutation::KernelFeatures(bits)));
    }
    samples
}

#[test]
fn block_and_transaction_decoders_accept_and_reject_the_same_samples() {
    let block = base_block();
    let transaction = transaction_with_body(block.body.clone());
    for (name, mutation) in body_samples() {
        check_block(&block, &name, &mutation);
        check_transaction(&transaction, &name, &mutation);
    }
}

#[test]
fn block_header_decoders_accept_and_reject_the_same_samples() {
    let block = base_block();
    let mut samples = Vec::new();
    for len in [0, 31, 32, 33] {
        samples.push((
            format!("block_output_mr of {len} bytes"),
            Mutation::HeaderBlockOutputMr(vec![7; len]),
        ));
    }
    for version in [0, 1, u32::from(u16::MAX), u32::from(u16::MAX) + 1] {
        samples.push((format!("header version {version:#x}"), Mutation::HeaderVersion(version)));
    }
    samples.extend(fixed_size_samples(
        "total kernel offset",
        random_scalar_bytes(),
        Mutation::HeaderTotalKernelOffset,
    ));
    for algo in [0, 1, 2, 3, 4, 0xff, 0x100] {
        samples.push((format!("pow algorithm {algo:#x}"), Mutation::HeaderPowAlgo(algo)));
    }
    for len in [0, 1, MAX_POW_DATA_SIZE, MAX_POW_DATA_SIZE + 1] {
        samples.push((
            format!("pow data of {len} bytes"),
            Mutation::HeaderPowData(vec![6; len]),
        ));
    }
    for (name, mutation) in samples {
        check_block(&block, &name, &mutation);
    }
}

#[test]
fn transaction_offset_decoders_accept_and_reject_the_same_samples() {
    let transaction = transaction_with_body(base_body());
    for (name, mutation) in fixed_size_samples("transaction offset", random_scalar_bytes(), Mutation::TransactionOffset)
    {
        check_transaction(&transaction, &name, &mutation);
    }
}

#[test]
fn encrypted_data_size_bounds_are_the_ones_the_samples_use() {
    // Keeps the samples meaningful if the bounds change
    assert!(EncryptedData::from_bytes(&[0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1]).is_err());
    assert!(EncryptedData::from_bytes(&[0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL]).is_ok());
    assert!(EncryptedData::from_bytes(&[0; MAX_ENCRYPTED_DATA_SIZE]).is_ok());
    assert!(EncryptedData::from_bytes(&[0; MAX_ENCRYPTED_DATA_SIZE + 1]).is_err());
}
