// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Measures the heap a decoded covenant keeps. This file is its own test binary with a counting global allocator, and
//! holds a single test so that nothing else allocates while it measures.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use borsh::BorshDeserialize;
use integer_encoding::VarIntWriter;
use tari_transaction_components::transaction_components::covenants::{
    Covenant,
    CovenantDecodeError,
    decode_borsh_covenant,
};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

fn record_alloc(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::SeqCst).saturating_add(size);
    PEAK.fetch_max(live, Ordering::SeqCst);
}

fn record_dealloc(size: usize) {
    LIVE.fetch_sub(size, Ordering::SeqCst);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        record_dealloc(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            record_dealloc(layout.size());
            record_alloc(new_size);
        }
        new_ptr
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// Covenant byte codes
const ARG_COVENANT: u8 = 0x05;
const ARG_OUTPUT_FIELD: u8 = 0x07;
const ARG_BYTES: u8 = 0x09;
const FILTER_FIELD_EQ: u8 = 0x33;
const FIELD_COVENANT: u8 = 0x03;

/// `prefix` followed by an `ARG_COVENANT` token holding `inner`
fn wrap(prefix: &[u8], inner: &[u8]) -> Vec<u8> {
    let mut buf = prefix.to_vec();
    buf.push(ARG_COVENANT);
    buf.write_varint(inner.len()).unwrap();
    buf.extend_from_slice(inner);
    buf
}

/// The borsh encoding of a covenant: a varint length and the covenant bytes
fn borsh_field(bytes: &[u8]) -> Vec<u8> {
    let mut field = Vec::new();
    field.write_varint(bytes.len()).unwrap();
    field.extend_from_slice(bytes);
    field
}

#[test]
fn covenant_decode_retains_only_encoded_bytes() {
    // 16 levels of nesting around `field_eq(@field::covenant, @bytes(3900 bytes))`
    let payload = [7u8; 3900];
    let mut covenant = vec![FILTER_FIELD_EQ, ARG_OUTPUT_FIELD, FIELD_COVENANT, ARG_BYTES];
    covenant.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
    covenant.extend_from_slice(&payload);
    for _ in 0..16 {
        covenant = wrap(&[], &covenant);
    }
    assert!(covenant.len() <= 4096);
    let field = borsh_field(&covenant);

    let before = LIVE.load(Ordering::SeqCst);
    let decoded = Covenant::deserialize(&mut field.as_slice()).unwrap();
    let retained = LIVE.load(Ordering::SeqCst).saturating_sub(before);
    assert_eq!(decoded.as_bytes(), covenant.as_slice());
    assert!(retained < 8 * 1024, "decoded covenant retained {retained} bytes");
    drop(decoded);

    // 16 levels of `ARG_OUTPUT_FIELD` tokens: 126 per level and an `ARG_COVENANT`, and 128 in the innermost level.
    // Every level is within the old per-level limit, but the total is far over the limit.
    let field_token = [ARG_OUTPUT_FIELD, FIELD_COVENANT];
    let mut covenant = field_token.repeat(128);
    for _ in 0..15 {
        covenant = wrap(&field_token.repeat(126), &covenant);
    }
    assert_eq!(covenant.len(), 4081);
    let field = borsh_field(&covenant);
    assert!(matches!(
        decode_borsh_covenant(&mut field.as_slice()),
        Err(CovenantDecodeError::ExceededMaxTokens { .. })
    ));

    let before = LIVE.load(Ordering::SeqCst);
    PEAK.store(before, Ordering::SeqCst);
    let err = Covenant::deserialize(&mut field.as_slice()).unwrap_err();
    let peak = PEAK.load(Ordering::SeqCst).saturating_sub(before);
    assert!(
        err.to_string().contains("exceeded the maximum of 128 tokens"),
        "unexpected error: {err}"
    );
    assert!(peak < 64 * 1024, "decoding peaked at {peak} bytes");
}
