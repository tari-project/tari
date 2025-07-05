//! # C Structure Representations for Transaction Data
//!
//! This module contains C-compatible structure definitions that mirror the 
//! transaction structures from the Tari wallet C FFI. These are used for 
//! safe casting and data extraction from void pointers in callbacks.

use std::os::raw::{c_char, c_longlong, c_ulonglong};

/// C representation of TariCompletedTransaction structure
/// This matches the layout from the Tari wallet C FFI
#[repr(C)]
#[derive(Debug)]
pub struct TariCompletedTransaction {
    pub tx_id: c_ulonglong,
    pub source_pk: *const TariWalletAddress,
    pub dest_pk: *const TariWalletAddress,
    pub amount: c_ulonglong,
    pub fee: c_ulonglong,
    pub message: *const c_char,
    pub timestamp: c_longlong,
    pub status: c_ulonglong,
    pub direction: c_ulonglong,
    pub excess_sig: *const TariComAndPubSignature,
    pub kernel: *const TariTransactionKernel,
}

/// C representation of TariPendingInboundTransaction structure  
/// This matches the layout from the Tari wallet C FFI
#[repr(C)]
#[derive(Debug)]
pub struct TariPendingInboundTransaction {
    pub tx_id: c_ulonglong,
    pub source_pk: *const TariWalletAddress,
    pub amount: c_ulonglong,
    pub message: *const c_char,
    pub timestamp: c_longlong,
    pub status: c_ulonglong,
}

/// Opaque pointer type for wallet addresses
/// We don't need the full structure, just safe pointer handling
#[repr(C)]
pub struct TariWalletAddress {
    _private: [u8; 0],
}

/// Opaque pointer type for signatures
/// We don't need the full structure, just safe pointer handling  
#[repr(C)]
pub struct TariComAndPubSignature {
    _private: [u8; 0],
}

/// Opaque pointer type for transaction kernels
/// We don't need the full structure, just safe pointer handling
#[repr(C)]
pub struct TariTransactionKernel {
    _private: [u8; 0],
}
