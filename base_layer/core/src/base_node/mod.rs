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

//! The Minotari base node implementation.
//!
//! Base nodes are the key pieces of infrastructure that maintain the security and integrity of the Tari
//! cryptocurrency. The role of the base node is to provide the following services:
//! * New transaction validation
//! * New block validation
//! * Chain synchronisation service
//! * A gRPC API exposing metrics and data about the blockchain state
//!
//! More details about the implementation are presented in
//! [RFC-0111](https://rfc.tari.com/RFC-0111_BaseNodeArchitecture.html).

pub mod chain_metadata_service;

pub mod comms_interface;

pub use comms_interface::LocalNodeCommsInterface;
#[cfg(feature = "metrics")]
mod metrics;

pub mod service;

pub mod state_machine_service;

pub use state_machine_service::{BaseNodeStateMachine, BaseNodeStateMachineConfig, StateMachineHandle};

pub mod sync;

pub use sync::{
    BlockchainSyncConfig,
    SyncValidators,
    rpc::{BaseNodeSyncService, create_base_node_sync_rpc_service},
};

pub mod proto;

pub mod rpc;

pub mod tari_pulse_service;
