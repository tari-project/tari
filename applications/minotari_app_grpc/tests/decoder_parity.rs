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
use tari_common_types::types::{CompressedPublicKey, FixedHash, PrivateKey};
use tari_core::{blocks::genesis_block::get_genesis_block, proto};
use tari_crypto::keys::SecretKey;
use tari_node_components::blocks::Block;
use tari_script::{ExecutionStack, MAX_SCRIPT_BYTES, StackItem, script};
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    covenant,
    transaction_components::{
        EncryptedData,
        Transaction,
        TransactionInput,
        encrypted_data::{MAX_ENCRYPTED_DATA_SIZE, STATIC_ENCRYPTED_DATA_SIZE_TOTAL},
    },
};
use tari_utilities::hex::to_hex;

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
    OutputEncryptedData(Vec<u8>),
    OutputScript(Vec<u8>),
    OutputCovenant(Vec<u8>),
    InputData(Vec<u8>),
    KernelFeatures(u32),
    /// Block only
    HeaderBlockOutputMr(Vec<u8>),
}

fn random_public_key() -> CompressedPublicKey {
    CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()))
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

fn mutate_json(value: &mut Value, mutation: &Mutation) {
    let (pointer, new_value) = match mutation {
        Mutation::Nothing => return,
        Mutation::OutputEncryptedData(bytes) => ("/body/outputs/0/encrypted_data/data", Value::from(to_hex(bytes))),
        Mutation::OutputScript(bytes) => ("/body/outputs/0/script", Value::from(to_hex(bytes))),
        Mutation::OutputCovenant(bytes) => ("/body/outputs/0/covenant", Value::from(to_hex(bytes))),
        Mutation::InputData(bytes) => ("/body/inputs/0/input_data", Value::from(to_hex(bytes))),
        Mutation::KernelFeatures(bits) => ("/body/kernels/0/features", Value::from(*bits)),
        // `FixedHash` is a JSON array of numbers
        Mutation::HeaderBlockOutputMr(bytes) => ("/header/block_output_mr", Value::from(bytes.clone())),
    };
    assert!(value.pointer(pointer).is_some(), "{pointer} not found in {value}");
    *value.pointer_mut(pointer).unwrap() = new_value;
}

fn mutate_p2p_body(body: &mut proto::types::AggregateBody, mutation: &Mutation) {
    match mutation {
        Mutation::Nothing | Mutation::HeaderBlockOutputMr(_) => {},
        Mutation::OutputEncryptedData(bytes) => body.outputs[0].encrypted_data = bytes.clone(),
        Mutation::OutputScript(bytes) => body.outputs[0].script = bytes.clone(),
        // The covenant field holds the borsh encoding of the covenant
        Mutation::OutputCovenant(bytes) => body.outputs[0].covenant = varint_prefixed(bytes),
        Mutation::InputData(bytes) => body.inputs[0].input_data = bytes.clone(),
        Mutation::KernelFeatures(bits) => body.kernels[0].features = *bits,
    }
}

fn mutate_grpc_body(body: &mut grpc::AggregateBody, mutation: &Mutation) {
    match mutation {
        Mutation::Nothing | Mutation::HeaderBlockOutputMr(_) => {},
        Mutation::OutputEncryptedData(bytes) => body.outputs[0].encrypted_data = bytes.clone(),
        Mutation::OutputScript(bytes) => body.outputs[0].script = bytes.clone(),
        // The covenant field holds the borsh encoding of the covenant
        Mutation::OutputCovenant(bytes) => body.outputs[0].covenant = varint_prefixed(bytes),
        Mutation::InputData(bytes) => body.inputs[0].input_data = bytes.clone(),
        Mutation::KernelFeatures(bits) => body.kernels[0].features = *bits,
    }
}

/// The encodings of a changed field before and after the change: `(borsh_before, borsh_after, bincode_before,
/// bincode_after)`
type BinaryMutation = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

/// The [`BinaryMutation`] of `mutation`. `None` if the binary formats can not express the change (a fixed size field
/// of the wrong size, or a kernel feature value that does not fit in a byte).
fn binary_mutation(body: &AggregateBody, mutation: &Mutation) -> Option<BinaryMutation> {
    let output = body.outputs().first().unwrap();
    let input = body.inputs().first().unwrap();
    let kernel = body.kernels().first().unwrap();
    match mutation {
        Mutation::Nothing => Some(Default::default()),
        Mutation::HeaderBlockOutputMr(_) => None,
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

fn check_block(block: &Block, name: &str, mutation: &Mutation) {
    let sample = format!("block {name}");

    let mut json = serde_json::to_value(block).unwrap();
    mutate_json(&mut json, mutation);
    let mut p2p = proto::core::Block::try_from(block.clone()).unwrap();
    mutate_p2p_body(p2p.body.as_mut().unwrap(), mutation);
    let mut grpc_block = grpc::Block::try_from(block.clone()).unwrap();
    mutate_grpc_body(grpc_block.body.as_mut().unwrap(), mutation);
    if let Mutation::HeaderBlockOutputMr(bytes) = mutation {
        p2p.header.as_mut().unwrap().block_output_mr = bytes.clone();
        grpc_block.header.as_mut().unwrap().block_output_mr = bytes.clone();
    }

    let mut results = vec![
        ("serde_json", serde_json::from_value::<Block>(json).ok()),
        ("P2P proto", Block::try_from(p2p).ok()),
        ("gRPC proto", Block::try_from(grpc_block).ok()),
    ];
    if let Some((borsh_before, borsh_after, bincode_before, bincode_after)) = binary_mutation(&block.body, mutation) {
        let borsh_encoding = splice(&borsh::to_vec(block).unwrap(), &borsh_before, &borsh_after);
        let bincode_encoding = splice(&bincode::serialize(block).unwrap(), &bincode_before, &bincode_after);
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

fn check_transaction(transaction: &Transaction, name: &str, mutation: &Mutation) {
    let sample = format!("transaction {name}");

    let mut json = serde_json::to_value(transaction).unwrap();
    mutate_json(&mut json, mutation);
    let mut p2p = proto::types::Transaction::try_from(transaction.clone()).unwrap();
    mutate_p2p_body(p2p.body.as_mut().unwrap(), mutation);
    let mut grpc_transaction = grpc::Transaction::try_from(transaction.clone()).unwrap();
    mutate_grpc_body(grpc_transaction.body.as_mut().unwrap(), mutation);

    // `Transaction` has no borsh encoding
    let mut results = vec![
        ("serde_json", serde_json::from_value::<Transaction>(json).ok()),
        ("P2P proto", Transaction::try_from(p2p).ok()),
        ("gRPC proto", Transaction::try_from(grpc_transaction).ok()),
    ];
    if let Some((_, _, bincode_before, bincode_after)) = binary_mutation(&transaction.body, mutation) {
        let bincode_encoding = splice(
            &bincode::serialize(transaction).unwrap(),
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

/// Body samples, shared by blocks and transactions. Each is named after the change it makes.
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
    for len in [0, 31, 32, 33] {
        check_block(
            &block,
            &format!("block_output_mr of {len} bytes"),
            &Mutation::HeaderBlockOutputMr(vec![7; len]),
        );
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
