//  Copyright 2023. The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use taiji_common::{
    configuration::Network,
    exit_codes::{ExitCode, ExitError},
};
use taiji_features::resolver::Target;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkCheckError {
    #[error("The network {0} is invalid for this binary built for MainNet")]
    MainNetBinary(Network),
    #[error("The network {0} is invalid for this binary built for NextNet")]
    NextNetBinary(Network),
    #[error("The network {0} is invalid for this binary built for TestNet")]
    TestNetBinary(Network),
}

impl From<NetworkCheckError> for ExitError {
    fn from(err: NetworkCheckError) -> Self {
        Self::new(ExitCode::NetworkError, err)
    }
}

#[cfg(taiji_network_mainnet)]
pub const TARGET_NETWORK: Target = Target::MainNet;

#[cfg(taiji_network_nextnet)]
pub const TARGET_NETWORK: Target = Target::NextNet;

#[cfg(all(not(taiji_network_mainnet), not(taiji_network_nextnet)))]
pub const TARGET_NETWORK: Target = Target::TestNet;

pub fn is_network_choice_valid(network: Network) -> Result<(), NetworkCheckError> {
    match (TARGET_NETWORK, network) {
        (Target::MainNet, Network::MainNet | Network::StageNet) => Ok(()),
        (Target::MainNet, _) => Err(NetworkCheckError::MainNetBinary(network)),

        (Target::NextNet, Network::NextNet) => Ok(()),
        (Target::NextNet, _) => Err(NetworkCheckError::NextNetBinary(network)),

        (Target::TestNet, Network::LocalNet | Network::Igor | Network::Esmeralda) => Ok(()),
        (Target::TestNet, _) => Err(NetworkCheckError::TestNetBinary(network)),
    }
}
