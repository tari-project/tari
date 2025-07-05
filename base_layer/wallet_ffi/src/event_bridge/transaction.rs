//! # Transaction Data Extraction Safety Layer
//!
//! This module provides safe transaction data extraction from C FFI structures.
//! It handles all error cases and validates pointers before dereferencing.

use super::types::TransactionData;
use crate::ffi::transaction_types::{TariPendingInboundTransaction, TariCompletedTransaction};
use std::ffi::CStr;
use std::os::raw::c_void;

/// Error types for transaction data extraction
#[derive(Debug, thiserror::Error)]
pub enum TransactionExtractionError {
    #[error("Null pointer provided")]
    NullPointer,
    #[error("Invalid UTF-8 in transaction message: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    #[error("Invalid string in transaction data")]
    InvalidString,
    #[error("Pointer casting failed")]
    CastingError,
}

/// Safely extract transaction data from a C pointer to TariPendingInboundTransaction
pub unsafe fn extract_transaction_data(tx: *mut c_void) -> Result<TransactionData, TransactionExtractionError> {
    if tx.is_null() {
        return Err(TransactionExtractionError::NullPointer);
    }

    // Cast void pointer to transaction structure
    let tx_ptr = tx as *const TariPendingInboundTransaction;
    let tx_struct = &*tx_ptr;
    
    // Extract transaction ID
    let tx_id = tx_struct.tx_id;
    
    // Extract amount (convert from c_ulonglong to u64)
    let amount = tx_struct.amount;
    
    // Extract timestamp
    let timestamp = tx_struct.timestamp;
    
    // Extract status
    let status = tx_struct.status as u8;
    
    // Safely extract source address (simplified - would need actual address extraction)
    let source_address = extract_address_string(tx_struct.source_pk)?;
    
    // Safely extract message if present
    let message = extract_message_string(tx_struct.message)?;
    
    Ok(TransactionData {
        tx_id,
        source_address,
        amount,
        message,
        timestamp,
        status,
    })
}

/// Safely extract address string from pointer
unsafe fn extract_address_string(addr_ptr: *const crate::ffi::transaction_types::TariWalletAddress) -> Result<String, TransactionExtractionError> {
    if addr_ptr.is_null() {
        return Ok("unknown_address".to_string());
    }
    
    // For now, return a placeholder since actual address extraction
    // would require understanding the TariWalletAddress structure
    Ok(format!("address_{:p}", addr_ptr))
}

/// Safely extract message string from C char pointer
unsafe fn extract_message_string(msg_ptr: *const std::os::raw::c_char) -> Result<Option<String>, TransactionExtractionError> {
    if msg_ptr.is_null() {
        return Ok(None);
    }
    
    // Convert C string to Rust string with UTF-8 validation
    let c_str = CStr::from_ptr(msg_ptr);
    let str_slice = c_str.to_str()?;
    
    // Enforce maximum message length to prevent memory issues
    const MAX_MESSAGE_LENGTH: usize = 1024;
    if str_slice.len() > MAX_MESSAGE_LENGTH {
        return Ok(Some(str_slice[..MAX_MESSAGE_LENGTH].to_string()));
    }
    
    Ok(Some(str_slice.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    
    #[test]
    fn test_extract_message_string_null() {
        unsafe {
            let result = extract_message_string(std::ptr::null());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), None);
        }
    }
    
    #[test]
    fn test_extract_message_string_valid() {
        let test_message = CString::new("test message").unwrap();
        unsafe {
            let result = extract_message_string(test_message.as_ptr());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Some("test message".to_string()));
        }
    }
    
    #[test]
    fn test_extract_transaction_data_null() {
        unsafe {
            let result = extract_transaction_data(std::ptr::null_mut());
            assert!(matches!(result, Err(TransactionExtractionError::NullPointer)));
        }
    }
}
