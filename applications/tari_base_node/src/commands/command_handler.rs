// Copyright 2020. The Tari Project
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
    cmp,
    io::{self, Write},
    ops::Deref,
    str::FromStr,
    string::ToString,
    sync::Arc,
    time::Instant,
};

use anyhow::{anyhow, Error};
use log::*;
use strum::{Display, EnumString};
use tari_app_utilities::utilities::parse_emoji_id_or_public_key;
use tari_common::GlobalConfig;
use tari_common_types::{emoji::EmojiId, types::HashOutput};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::PeerManager,
    protocol::rpc::RpcServerHandle,
    NodeIdentity,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester, MetricsCollectorHandle};
use tari_core::{
    base_node::{state_machine_service::states::StatusInfo, LocalNodeCommsInterface},
    chain_storage::{async_db::AsyncBlockchainDb, LMDBDatabase},
    consensus::ConsensusManager,
    mempool::service::LocalMempoolService,
    proof_of_work::PowAlgorithm,
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_p2p::{auto_update::SoftwareUpdaterHandle, services::liveness::LivenessHandle};
use tari_utilities::{hex::Hex, message_format::MessageFormat, Hashable};
use thiserror::Error;
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

use crate::builder::BaseNodeContext;

#[derive(Debug, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum StatusLineOutput {
    Log,
    StdOutAndLog,
}

#[derive(Debug, Error)]
#[error("invalid format '{0}'")]
pub struct FormatParseError(String);

#[derive(Debug, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum Format {
    Json,
    Text,
}

impl Default for Format {
    fn default() -> Self {
        Self::Text
    }
}
