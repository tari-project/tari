//  Copyright 2021. The Tari Project
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

use std::{env, path::PathBuf};

pub struct Settings {
  pub(crate) wallet_grpc_address: String,
  pub(crate) base_node_grpc_address: String,
  pub(crate) validator_node_grpc_address: String,
  pub(crate) data_dir: PathBuf,
}

impl Settings {
  pub fn new() -> Self {
    // Self {
    //   wallet_grpc_address: "localhost:18143".to_string(),
    //   base_node_grpc_address: "localhost:18142".to_string(),
    //   _favourite_assets: vec!["1234".to_string()],
    // }
    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let data_dir = PathBuf::from(data_dir);
    // TODO: remove this, just for convenience
    Self {
      wallet_grpc_address: env::var("WALLET_GRPC_ADDRESS")
        .unwrap_or_else(|_| "localhost:18143".to_string()),
      base_node_grpc_address: env::var("BASE_NODE_GRPC_ADDRESS")
        .unwrap_or_else(|_| "localhost:18142".to_string()),
      validator_node_grpc_address: env::var("VALIDATOR_NODE_GRPC_ADDRESS")
        .unwrap_or_else(|_| "localhost:18144".to_string()),
      data_dir,
    }
  }
}
