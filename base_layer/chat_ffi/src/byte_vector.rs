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

use std::{ptr, slice};

use libc::{c_int, c_uchar, c_uint};

use crate::error::{InterfaceError, LibChatError};

#[derive(Debug, PartialEq, Clone)]
pub struct ChatByteVector(pub Vec<c_uchar>); // declared like this so that it can be exposed to external header

/// Creates a ChatByteVector
///
/// ## Arguments
/// `byte_array` - The pointer to the byte array
/// `element_count` - The number of elements in byte_array
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut ChatByteVector` - Pointer to the created ChatByteVector. Note that it will be ptr::null_mut()
/// if the byte_array pointer was null or if the elements in the byte_vector don't match
/// element_count when it is created
///
/// # Safety
/// The ```byte_vector_destroy``` function must be called when finished with a ChatByteVector to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn chat_byte_vector_create(
    byte_array: *const c_uchar,
    element_count: c_uint,
    error_out: *mut c_int,
) -> *mut ChatByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let mut bytes = ChatByteVector(Vec::new());
    if byte_array.is_null() {
        error = LibChatError::from(InterfaceError::NullError("byte_array".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        let array: &[c_uchar] = slice::from_raw_parts(byte_array, element_count as usize);
        bytes.0 = array.to_vec();
        if bytes.0.len() != element_count as usize {
            error = LibChatError::from(InterfaceError::AllocationError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        }
    }
    Box::into_raw(Box::new(bytes))
}

/// Frees memory for a ChatByteVector
///
/// ## Arguments
/// `bytes` - The pointer to a ChatByteVector
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn chat_byte_vector_destroy(bytes: *mut ChatByteVector) {
    if !bytes.is_null() {
        drop(Box::from_raw(bytes))
    }
}

/// Gets a c_uchar at position in a ChatByteVector
///
/// ## Arguments
/// `ptr` - The pointer to a ChatByteVector
/// `position` - The integer position
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uchar` - Returns a character. Note that the character will be a null terminator (0) if ptr
/// is null or if the position is invalid
///
/// # Safety
/// None
// converting between here is fine as its used to clamp the the array to length
#[allow(clippy::cast_possible_wrap)]
#[no_mangle]
pub unsafe extern "C" fn chat_byte_vector_get_at(
    ptr: *mut ChatByteVector,
    position: c_uint,
    error_out: *mut c_int,
) -> c_uchar {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if ptr.is_null() {
        error = LibChatError::from(InterfaceError::NullError("ptr".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0u8;
    }
    let len = chat_byte_vector_get_length(ptr, error_out) as c_int - 1; // clamp to length
    if len < 0 || position > len as c_uint {
        error = LibChatError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0u8;
    }

    (*ptr).0[position as usize]
}

/// Gets the number of elements in a ChatByteVector
///
/// ## Arguments
/// `ptr` - The pointer to a ChatByteVector
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `c_uint` - Returns the integer number of elements in the ChatByteVector. Note that it will be zero
/// if ptr is null
///
/// # Safety
/// None
// casting here is okay a byte vector wont go larger than u32
#[allow(clippy::cast_possible_truncation)]
#[no_mangle]
pub unsafe extern "C" fn chat_byte_vector_get_length(vec: *const ChatByteVector, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    if vec.is_null() {
        error = LibChatError::from(InterfaceError::NullError("vec".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*vec).0.len() as c_uint
}

pub(crate) unsafe fn process_vector(vector: *mut ChatByteVector, error_out: *mut c_int) -> Vec<u8> {
    let data_byte_vector_length = chat_byte_vector_get_length(vector, error_out);
    let mut bytes: Vec<u8> = Vec::new();

    if data_byte_vector_length > 0 {
        for c in 0..data_byte_vector_length {
            let byte = chat_byte_vector_get_at(vector, c as c_uint, error_out);
            bytes.push(byte);
        }
    }

    bytes
}
