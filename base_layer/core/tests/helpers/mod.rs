//! This module provides a large set of useful functions and utilities for creating and playing with aspects of the
//! Tari base blockchain.
//! There are macros, such as `txn_schema!` that help you to easily construct valid transactions in test blockchains,
//! through to functions that bootstrap entire blockchains in `sample_blockchains`.

pub mod block_builders;
pub mod event_stream;
pub mod nodes;
pub mod sample_blockchains;
