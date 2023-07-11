// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! This module provides a large set of useful functions and utilities for creating and playing with aspects of the
//! Tari base blockchain.
//! There are macros, such as `txn_schema!` that help you to easily construct valid transactions in test blockchains,
//! through to functions that bootstrap entire blockchains in `sample_blockchains`.

#[cfg(any(test))]
pub mod block_builders;
#[cfg(any(test))]
pub mod block_malleability;
#[cfg(any(test))]
pub mod block_proxy;
#[cfg(any(test))]
pub mod chain_metadata;
#[cfg(any(test))]
pub mod database;
#[cfg(any(test))]
pub mod event_stream;
#[cfg(any(test))]
pub mod mock_state_machine;
#[cfg(any(test))]
pub mod nodes;
#[cfg(any(test))]
pub mod sample_blockchains;
#[cfg(any(test))]
pub mod test_block_builder;
#[cfg(any(test))]
pub mod test_blockchain;
