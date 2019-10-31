// Copyright 2019, The Tari Project
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

use super::protocol as proto;

use crate::transaction_protocol::TransactionMetadata;
use std::convert::TryFrom;

impl From<proto::TransactionMetadata> for TransactionMetadata {
    fn from(metadata: proto::TransactionMetadata) -> Self {
        Self {
            fee: metadata.fee.into(),
            lock_height: metadata.lock_height,
            meta_info: metadata.meta_info.map(Into::into),
            linked_kernel: metadata.linked_kernel.map(Into::into),
        }
    }
}

impl From<TransactionMetadata> for proto::TransactionMetadata {
    fn from(metadata: TransactionMetadata) -> Self {
        Self {
            // The absolute fee for the transaction
            fee: metadata.fee.into(),
            // The earliest block this transaction can be mined
            lock_height: metadata.lock_height,
            // This is an optional field used by committing to additional tx meta data between the two parties
            meta_info: metadata.meta_info.map(Into::into),
            // This is an optional field and is the hash of the kernel this kernel is linked to.
            // This field is for example for relative time-locked transactions
            linked_kernel: metadata.linked_kernel.map(Into::into),
        }
    }
}
