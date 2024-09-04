//  Copyright 2024. The Tari Project
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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),
    #[error("Common config error: {0}")]
    CommonConfig(#[from] tari_common::configuration::error::ConfigError),
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Request error: {0}")]
    Request(#[from] RequestError),
    #[error("Mining cycle error: {0}")]
    Mining(#[from] MiningError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing base node or proxy address")]
    MissingBaseNode,
    #[error("Missing monero wallet address")]
    MissingMoneroWalletAddress,
    #[error("Common config error: {0}")]
    CommonConfig(#[from] tari_common::configuration::error::ConfigError),
}

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("Failed to process request `get_block_count`: {0}")]
    GetBlockCount(String),
    #[error("Failed to process request `get_block_template`: {0}")]
    GetBlockTemplate(String),
    #[error("Failed to process request `submit_block`: {0}")]
    SubmitBlock(String),
}

#[derive(Debug, thiserror::Error)]
pub enum MiningError {
    #[error("DifficultyError`: {0}")]
    Difficulty(#[from] tari_core::proof_of_work::DifficultyError),
    #[error("RandomXVMFactoryError`: {0}")]
    RandomXVMFactory(#[from] tari_core::proof_of_work::randomx_factory::RandomXVMFactoryError),
    #[error("HexError`: {0}")]
    Hex(#[from] tari_utilities::hex::HexError),
    #[error("FromHexError`: {0}")]
    FromHex(#[from] hex::FromHexError),
    #[error("MergeMineError`: {0}")]
    MergeMine(#[from] tari_core::proof_of_work::monero_rx::MergeMineError),
    #[error("Request error: {0}")]
    Request(#[from] RequestError),
    #[error("RandomXError: {0}")]
    RandomX(#[from] randomx_rs::RandomXError),
    #[error("Tokio runtime error: {0}")]
    TokioRuntime(String),
    #[error("Dataset error: {0}")]
    Dataset(#[from] DatasetError),
}

#[derive(Debug, thiserror::Error)]
pub enum DatasetError {
    #[error("Read lock error: {0}")]
    ReadLock(String),
    #[error("Write lock error: {0}")]
    WriteLock(String),
    #[error("RandomXError: {0}")]
    RandomX(#[from] randomx_rs::RandomXError),
    #[error("Dataset could not be found or created")]
    DatasetNotFound,
    #[error("FromHexError`: {0}")]
    FromHex(#[from] hex::FromHexError),
}
