// Copyright 2021. The Tari Project
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

use std::fmt::{Display, Error, Formatter};

use tari_core::{
    consensus::ConsensusConstants,
    transactions::{transaction_protocol::RewindData, CryptoFactories},
};
use tari_shutdown::ShutdownSignal;

use crate::output_manager_service::{
    config::OutputManagerServiceConfig,
    handle::OutputManagerEventSender,
    storage::database::OutputManagerDatabase,
};

/// This struct is a collection of the common resources that a async task in the service requires.
#[derive(Clone)]
pub(crate) struct OutputManagerResources<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    pub config: OutputManagerServiceConfig,
    pub db: OutputManagerDatabase<TBackend>,
    pub factories: CryptoFactories,
    pub event_publisher: OutputManagerEventSender,
    pub master_key_manager: TKeyManagerInterface,
    pub consensus_constants: ConsensusConstants,
    pub connectivity: TWalletConnectivity,
    pub shutdown_signal: ShutdownSignal,
    pub rewind_data: RewindData,
}

#[derive(Clone, Copy)]
pub enum OutputManagerKeyManagerBranch {
    Spend,
    SpendScript,
    Coinbase,
    CoinbaseScript,
    RecoveryViewOnly,
    RecoveryBlinding,
    RecoveryByte,
}

impl Display for OutputManagerKeyManagerBranch {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let response = match self {
            OutputManagerKeyManagerBranch::Spend => "Spend",
            OutputManagerKeyManagerBranch::SpendScript => "Script",
            OutputManagerKeyManagerBranch::Coinbase => "Coinbase",
            OutputManagerKeyManagerBranch::CoinbaseScript => "Coinbase_script",
            OutputManagerKeyManagerBranch::RecoveryViewOnly => "Recovery_viewonly",
            OutputManagerKeyManagerBranch::RecoveryBlinding => "Recovery_blinding",
            OutputManagerKeyManagerBranch::RecoveryByte => "Recovery_byte",
        };
        fmt.write_str(response)
    }
}

#[allow(clippy::from_over_into)]
impl Into<String> for OutputManagerKeyManagerBranch {
    fn into(self) -> String {
        self.to_string()
    }
}
