//  Copyright 2022. The Tari Project
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



use std::ffi::CString;
use libc::c_char;
use minotari_wallet::output_manager_service::storage::models::DbWalletOutput;
use minotari_wallet::output_manager_service::storage::OutputStatus;
use tari_utilities::hex::Hex;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct TariUtxo {
    pub commitment: *const c_char,
    pub value: u64,
    pub mined_height: u64,
    pub mined_timestamp: u64,
    pub status: u8,
}

impl From<DbWalletOutput> for TariUtxo {
    fn from(x: DbWalletOutput) -> Self {
        Self {
            commitment: CString::new(x.commitment.to_hex())
                .expect("failed to obtain hex from a commitment")
                .into_raw(),
            value: x.wallet_output.value.as_u64(),
            mined_height: x.mined_height.unwrap_or(0),
            mined_timestamp: x
                .mined_timestamp
                .map(|ts| ts.timestamp_millis() as u64)
                .unwrap_or_default(),
            status: match x.status {
                OutputStatus::Unspent => 0,
                OutputStatus::Spent => 1,
                OutputStatus::EncumberedToBeReceived => 2,
                OutputStatus::EncumberedToBeSpent => 3,
                OutputStatus::Invalid => 4,
                OutputStatus::CancelledInbound => 5,
                OutputStatus::UnspentMinedUnconfirmed => 6,
                OutputStatus::ShortTermEncumberedToBeReceived => 7,
                OutputStatus::ShortTermEncumberedToBeSpent => 8,
                OutputStatus::SpentMinedUnconfirmed => 9,
                OutputStatus::NotStored => 10,
            },
        }
    }
}
