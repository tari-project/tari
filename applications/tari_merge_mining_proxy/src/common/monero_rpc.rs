//  Copyright 2021, The Tari Project
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

//! # Monero helpers and constants
//!
//! ## Consts
//!
//! Response codes taken from https://github.com/monero-project/monero/blob/8286f07b265d16a87b3fe3bb53e8d7bf37b5265a/src/rpc/core_rpc_server_error_codes.h

// Even though we don't construct all variants, we want a complete list of them.
#[allow(dead_code)]
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub enum CoreRpcErrorCode {
    WrongParam = -1,
    TooBigHeight = -2,
    TooBigReserveSize = -3,
    WrongWalletAddress = -4,
    InternalError = -5,
    WrongBlockblob = -6,
    BlockNotAccepted = -7,
    CoreBusy = -9,
    WrongBlockblobSize = -10,
    UnsupportedRpc = -11,
    MiningToSubaddress = -12,
    RegtestRequired = -13,
    PaymentRequired = -14,
    InvalidClient = -15,
    PaymentTooLow = -16,
    DuplicatePayment = -17,
    StalePayment = -18,
    Restricted = -19,
}

impl CoreRpcErrorCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl Into<i32> for CoreRpcErrorCode {
    fn into(self) -> i32 {
        self.as_i32()
    }
}
