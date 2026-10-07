// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Tests for what decoding RPC payloads of transactions and block bodies can cost.

use std::mem::size_of;

use crate::proto;

/// prost allocates one struct per decoded input/output, so their inline size is what the decode budget multiplies.
/// The large optional sub-messages are boxed in build.rs to keep it small (inputs were 776 bytes, outputs 608).
#[test]
fn boxed_sub_messages_keep_inputs_and_outputs_small() {
    assert!(
        size_of::<proto::types::TransactionInput>() <= 256,
        "TransactionInput is {} bytes",
        size_of::<proto::types::TransactionInput>()
    );
    assert!(
        size_of::<proto::types::TransactionOutput>() <= 192,
        "TransactionOutput is {} bytes",
        size_of::<proto::types::TransactionOutput>()
    );
}
