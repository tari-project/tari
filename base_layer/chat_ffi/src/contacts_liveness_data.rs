// Copyright 2023, The Tari Project
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

use std::ptr;

use libc::{c_int, c_longlong, c_uchar};
use tari_common_types::tari_address::TariAddress;
use tari_contacts::contacts_service::handle::ContactsLivenessData;

use crate::error::{InterfaceError, LibChatError};

/// Returns a pointer to a TariAddress
///
/// ## Arguments
/// `liveness` - A pointer to a ContactsLivenessData struct
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut TariAddress` - A ptr to a TariAddress
///
/// ## Safety
/// `liveness` should be destroyed eventually
/// the returned `TariAddress` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_liveness_data_address(
    liveness: *mut ContactsLivenessData,
    error_out: *mut c_int,
) -> *mut TariAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if liveness.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let address = (*liveness).address().clone();
    Box::into_raw(Box::new(address))
}

/// Returns an c_uchar representation of a contacts online status
///
/// ## Arguments
/// `liveness` - A pointer to a ContactsLivenessData struct
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_uchar` - A c_uchar rep of an enum for a contacts online status. May return 0 if an error occurs
///     Online => 1
///     Offline => 2
///     NeverSeen => 3
///     Banned => 4
///
/// ## Safety
/// `liveness` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_liveness_data_online_status(
    liveness: *mut ContactsLivenessData,
    error_out: *mut c_int,
) -> c_uchar {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if liveness.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*liveness).online_status().as_u8()
}

/// Returns an c_longlong representation of a timestamp when the contact was last seen
///
/// ## Arguments
/// `liveness` - A pointer to a ContactsLivenessData struct
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_longlong` - A c_longlong rep of an enum for a contacts online status. May return -1 if an error
/// occurs, or 0 if the contact has never been seen
///
/// ## Safety
/// `liveness` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_liveness_data_last_seen(
    liveness: *mut ContactsLivenessData,
    error_out: *mut c_int,
) -> c_longlong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if liveness.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    match (*liveness).last_ping_pong_received() {
        Some(last_seen) => last_seen.timestamp(),
        None => 0,
    }
}

/// Frees memory for a ContactsLivenessData
///
/// ## Arguments
/// `ptr` - The pointer of a ContactsLivenessData
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_contacts_liveness_data(ptr: *mut ContactsLivenessData) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use chrono::NaiveDateTime;
    use tari_contacts::contacts_service::service::{ContactMessageType, ContactOnlineStatus};
    use tari_utilities::epoch_time::EpochTime;

    use super::*;
    use crate::tari_address::destroy_tari_address;

    #[test]
    fn test_reading_address() {
        let address =
            TariAddress::from_hex("0c017c5cd01385f34ac065e3b05948326dc55d2494f120c6f459a07389011b4ec1").unwrap();
        let liveness = ContactsLivenessData::new(
            address.clone(),
            Default::default(),
            None,
            None,
            ContactMessageType::Ping,
            ContactOnlineStatus::Online,
        );
        let liveness_ptr = Box::into_raw(Box::new(liveness));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let address_ptr = read_liveness_data_address(liveness_ptr, error_out);

            assert_eq!(address.to_bytes(), (*address_ptr).to_bytes());

            destroy_contacts_liveness_data(liveness_ptr);
            destroy_tari_address(address_ptr);
            drop(Box::from_raw(error_out));
        }
    }

    #[test]
    fn test_reading_online_status() {
        let statuses = [
            ContactOnlineStatus::Online,
            ContactOnlineStatus::Offline,
            ContactOnlineStatus::Banned("banned".to_string()),
            ContactOnlineStatus::NeverSeen,
        ];
        for status in statuses {
            let liveness = ContactsLivenessData::new(
                Default::default(),
                Default::default(),
                None,
                None,
                ContactMessageType::Ping,
                status.clone(),
            );
            let liveness_ptr = Box::into_raw(Box::new(liveness));
            let error_out = Box::into_raw(Box::new(0));

            unsafe {
                let status_byte = read_liveness_data_online_status(liveness_ptr, error_out);

                assert_eq!(
                    status.clone().as_u8(),
                    status_byte,
                    "Testing status: {} but got {}",
                    status,
                    status_byte
                );

                destroy_contacts_liveness_data(liveness_ptr);
                drop(Box::from_raw(error_out));
            }
        }
    }

    #[test]
    fn test_reading_online_status_with_no_ptr() {
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let status_byte = read_liveness_data_online_status(ptr::null_mut(), error_out);

            assert_eq!(0, status_byte);

            drop(Box::from_raw(error_out));
        }
    }

    #[test]
    fn test_reading_last_seen() {
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let timestamp = EpochTime::now().as_u64();
            let liveness = ContactsLivenessData::new(
                Default::default(),
                Default::default(),
                None,
                NaiveDateTime::from_timestamp_opt(i64::try_from(timestamp).unwrap(), 0),
                ContactMessageType::Ping,
                ContactOnlineStatus::Online,
            );
            let liveness_ptr = Box::into_raw(Box::new(liveness));
            let c_timestamp = read_liveness_data_last_seen(liveness_ptr, error_out);

            assert_eq!(timestamp, c_timestamp as u64);

            destroy_contacts_liveness_data(liveness_ptr);
        }

        unsafe {
            let liveness = ContactsLivenessData::new(
                Default::default(),
                Default::default(),
                None,
                None,
                ContactMessageType::Ping,
                ContactOnlineStatus::Online,
            );
            let liveness_ptr = Box::into_raw(Box::new(liveness));
            let c_timestamp = read_liveness_data_last_seen(liveness_ptr, error_out);

            assert_eq!(0, c_timestamp as u64);

            destroy_contacts_liveness_data(liveness_ptr);
        }

        unsafe {
            drop(Box::from_raw(error_out));
        }
    }
}
