// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Shared helpers used by the bounded collection types to enforce their `MAX` bound while
//! deserializing.
//!
//! The bounded types (`MaxSizeBytes`, `MaxSizeVec` and `MaxSizeString`) must not derive
//! `BorshDeserialize`: the derived implementation reads the inner collection straight from its
//! length prefix and never checks the bound, so a value decoded off the wire could exceed `MAX`
//! even though every constructor rejects such a value. These helpers read (and validate) the
//! length prefix *before* any data is read, so an oversized payload is rejected up front.

use borsh::{
    BorshDeserialize,
    io::{Error, ErrorKind, Read, Result},
};

/// Message used when the decoded length prefix exceeds the type's maximum size.
pub(crate) const ERROR_MAX_SIZE_EXCEEDED: &str = "Length exceeds the maximum size of the type";
/// Matches the message borsh itself uses when the input ends before the declared length is read.
pub(crate) const ERROR_UNEXPECTED_LENGTH_OF_INPUT: &str = "Unexpected length of input";

/// Reads a borsh length prefix and validates it against `max` before any element is read.
///
/// This is deliberately done before reading the collection contents so that an attacker-supplied
/// length can never cause us to read (or allocate for) more than `max`.
pub(crate) fn read_checked_len<R: Read>(reader: &mut R, max: usize, type_name: &str) -> Result<usize> {
    let len = u32::deserialize_reader(reader)?;
    let len = usize::try_from(len).map_err(|_| Error::new(ErrorKind::InvalidData, ERROR_MAX_SIZE_EXCEEDED))?;
    if len > max {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("{type_name}: {ERROR_MAX_SIZE_EXCEEDED} ({len} > {max})"),
        ));
    }
    Ok(len)
}

/// Reads exactly `len` bytes from the reader.
///
/// `len` is expected to have been validated by [`read_checked_len`] already. The allocation is
/// still grown incrementally (mirroring borsh's own `Vec<u8>` implementation) so that a large
/// `MAX` cannot be used to force a large allocation from a small message.
pub(crate) fn read_bytes<R: Read>(reader: &mut R, len: usize) -> Result<Vec<u8>> {
    const CHUNK: usize = 4096;
    let mut buf = Vec::with_capacity(len.min(CHUNK));
    let mut chunk = [0u8; CHUNK];
    let mut remaining = len;
    while remaining > 0 {
        let take = remaining.min(CHUNK);
        let dst = chunk
            .get_mut(..take)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, ERROR_UNEXPECTED_LENGTH_OF_INPUT))?;
        reader
            .read_exact(dst)
            .map_err(|_| Error::new(ErrorKind::InvalidData, ERROR_UNEXPECTED_LENGTH_OF_INPUT))?;
        buf.extend_from_slice(dst);
        remaining -= take;
    }
    Ok(buf)
}

/// The initial capacity to allocate for a collection of `len` elements of type `T`.
///
/// Mirrors borsh's own (private) `hint::cautious`: the length is attacker controlled, so only a
/// bounded amount is allocated up front and the collection grows as elements are actually read.
pub(crate) fn cautious_capacity<T>(len: usize) -> usize {
    let el_size = core::mem::size_of::<T>().max(1);
    len.min((4096 / el_size).max(1))
}
