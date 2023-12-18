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


use std::ffi::{c_void, CStr, CString};
use std::mem::ManuallyDrop;
use itertools::Itertools;
use libc::c_char;
use minotari_wallet::output_manager_service::storage::models::DbWalletOutput;
use minotari_wallet::output_manager_service::storage::OutputStatus;
use tari_common_types::types::Commitment;
use crate::error::InterfaceError;
use tari_utilities::hex::Hex;
use crate::TariTypeTag;
use crate::TariUtxo;
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TariVector {
    pub tag: TariTypeTag,
    pub len: usize,
    pub cap: usize,
    pub ptr: *mut c_void,
}

impl From<Vec<i64>> for TariVector {
    fn from(v: Vec<i64>) -> Self {
        let mut v = ManuallyDrop::new(v);

        Self {
            tag: TariTypeTag::I64,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<u64>> for TariVector {
    fn from(v: Vec<u64>) -> Self {
        let mut v = ManuallyDrop::new(v);

        Self {
            tag: TariTypeTag::U64,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<String>> for TariVector {
    fn from(v: Vec<String>) -> Self {
        let mut v = ManuallyDrop::new(
            v.into_iter()
                .map(|x| CString::new(x.as_str()).unwrap().into_raw())
                .collect::<Vec<*mut c_char>>(),
        );

        Self {
            tag: TariTypeTag::Text,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<Commitment>> for TariVector {
    fn from(v: Vec<Commitment>) -> Self {
        let mut v = ManuallyDrop::new(
            v.into_iter()
                .map(|x| CString::new(x.to_hex().as_str()).unwrap().into_raw())
                .collect::<Vec<*mut c_char>>(),
        );

        Self {
            tag: TariTypeTag::Commitment,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<DbWalletOutput>> for TariVector {
    fn from(v: Vec<DbWalletOutput>) -> TariVector {
        let mut v = ManuallyDrop::new(v.into_iter().map(TariUtxo::from).collect_vec());

        Self {
            tag: TariTypeTag::Utxo,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

impl From<Vec<OutputStatus>> for TariVector {
    fn from(v: Vec<OutputStatus>) -> TariVector {
        let mut v = ManuallyDrop::new(v.into_iter().map(|x| x as i32 as u64).collect_vec());

        Self {
            tag: TariTypeTag::U64,
            len: v.len(),
            cap: v.capacity(),
            ptr: v.as_mut_ptr() as *mut c_void,
        }
    }
}

#[allow(dead_code)]
impl TariVector {
    pub(crate) fn to_string_vec(&self) -> Result<Vec<String>, InterfaceError> {
        if self.tag != TariTypeTag::Text {
            return Err(InterfaceError::InvalidArgument(format!(
                "expecting String, got {}",
                self.tag
            )));
        }

        if self.ptr.is_null() {
            return Err(InterfaceError::NullError(String::from(
                "tari vector of strings has null pointer",
            )));
        }

        Ok(unsafe {
            Vec::from_raw_parts(self.ptr as *mut *mut c_char, self.len, self.cap)
                .into_iter()
                .map(|x| {
                    CStr::from_ptr(x)
                        .to_str()
                        .expect("failed to convert from a vector of strings")
                        .to_string()
                })
                .collect()
        })
    }

    pub(crate) fn to_commitment_vec(&self) -> Result<Vec<Commitment>, InterfaceError> {
        self.to_string_vec()?
            .into_iter()
            .map(|x| {
                Commitment::from_hex(x.as_str())
                    .map_err(|e| InterfaceError::PointerError(format!("failed to convert hex to commitment: {:?}", e)))
            })
            .try_collect::<Commitment, Vec<Commitment>, InterfaceError>()
    }

    #[allow(dead_code)]
    pub(crate) fn to_utxo_vec(&self) -> Result<Vec<TariUtxo>, InterfaceError> {
        if self.tag != TariTypeTag::Utxo {
            return Err(InterfaceError::InvalidArgument(format!(
                "expecting Utxo, got {}",
                self.tag
            )));
        }

        if self.ptr.is_null() {
            return Err(InterfaceError::NullError(String::from(
                "tari vector of utxos has null pointer",
            )));
        }

        Ok(unsafe { Vec::from_raw_parts(self.ptr as *mut TariUtxo, self.len, self.cap) })
    }
}
