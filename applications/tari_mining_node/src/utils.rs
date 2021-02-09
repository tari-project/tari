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
//
use crate::errors::{err_empty, MinerError};
use tari_app_grpc::tari_rpc::{
    GetCoinbaseRequest,
    GetCoinbaseResponse,
    NewBlockTemplateResponse,
    TransactionKernel,
    TransactionOutput,
};

/// Convert NewBlockTemplateResponse to GetCoinbaseRequest
pub fn coinbase_request(template_response: &NewBlockTemplateResponse) -> Result<GetCoinbaseRequest, MinerError> {
    let template = template_response
        .new_block_template
        .as_ref()
        .ok_or_else(|| err_empty("new_block_template"))?;
    let miner_data = template_response
        .miner_data
        .as_ref()
        .ok_or_else(|| err_empty("miner_data"))?;
    let fee = miner_data.total_fees;
    let reward = miner_data.reward;
    let height = template
        .header
        .as_ref()
        .ok_or_else(|| err_empty("template.header"))?
        .height;
    Ok(GetCoinbaseRequest { height, fee, reward })
}

pub fn extract_outputs_and_kernels(
    coinbase: GetCoinbaseResponse,
) -> Result<(TransactionOutput, TransactionKernel), MinerError> {
    let transaction_body = coinbase
        .transaction
        .ok_or_else(|| err_empty("coinbase.transaction"))?
        .body
        .ok_or_else(|| err_empty("transaction.body"))?;
    let output = transaction_body
        .outputs
        .get(0)
        .cloned()
        .ok_or_else(|| err_empty("transaction.body.outputs"))?;
    let kernel = transaction_body
        .kernels
        .get(0)
        .cloned()
        .ok_or_else(|| err_empty("transaction.body.kernels"))?;
    Ok((output, kernel))
}
