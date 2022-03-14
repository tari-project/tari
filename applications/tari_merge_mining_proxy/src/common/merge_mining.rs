//  Copyright 2020, The Tari Project
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

use std::convert::{TryFrom, TryInto};

use tari_app_grpc::tari_rpc as grpc;
use tari_core::{
    blocks::NewBlockTemplate,
    transactions::transaction_components::{TransactionKernel, TransactionOutput},
};

use crate::error::MmProxyError;

pub fn add_coinbase(
    coinbase: grpc::Transaction,
    block_template: grpc::NewBlockTemplate,
) -> Result<grpc::NewBlockTemplate, MmProxyError> {
    let mut block_template = NewBlockTemplate::try_from(block_template)
        .map_err(|e| MmProxyError::MissingDataError(format!("GRPC Conversion Error: {}", e)))?;
    let output = TransactionOutput::try_from(coinbase.body.as_ref().unwrap().outputs[0].clone())
        .map_err(MmProxyError::MissingDataError)?;
    let kernel = TransactionKernel::try_from(coinbase.body.as_ref().unwrap().kernels[0].clone())
        .map_err(MmProxyError::MissingDataError)?;
    block_template.body.add_output(output);
    block_template.body.add_kernel(kernel);
    block_template.try_into().map_err(MmProxyError::ConversionError)
}
