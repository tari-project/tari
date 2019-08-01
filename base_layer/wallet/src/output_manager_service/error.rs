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

use derive_error::Error;
use tari_core::transaction_protocol::TransactionProtocolError;
use tari_utilities::ByteArrayError;

#[derive(Debug, Error, PartialEq)]
pub enum OutputManagerError {
    #[error(msg_embedded, no_from, non_std)]
    BuildError(String),
    ByteArrayError(ByteArrayError),
    TransactionProtocolError(TransactionProtocolError),
    /// If an pending transaction does not exist to be confirmed
    PendingTransactionNotFound,
    /// Not all the transaction inputs and outputs are present to be confirmed
    IncompleteTransaction,
    /// Not enough funds to fulfill transaction
    NotEnoughFunds,
    /// Output already exists
    DuplicateOutput,
    /// Error sending a message to the public API
    ApiSendFailed,
    /// Error receiving a message from the publcic API
    ApiReceiveFailed,
    /// API returned something unexpected.
    UnexpectedApiResponse,
}
