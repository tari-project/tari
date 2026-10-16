// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use integer_encoding::VarIntWriter;

use crate::transaction_components::covenants::{
    arguments::{CovenantArg, MAX_COVENANT_ARG_SIZE},
    byte_codes::ARG_COVENANT,
    covenant::{MAX_COVENANT_BYTES, MAX_COVENANT_DEPTH, MAX_COVENANT_TOKENS},
    decoder::{CovenantDecodeError, CovenantReadExt, read_canonical_len},
    filters::CovenantFilter,
};

/// Validates `bytes` as a covenant nested `depth` levels deep, appends its canonical encoding to `out`, and charges
/// every token (all levels) against `remaining`. Returns the number of top-level tokens.
pub(super) fn scan(
    bytes: &[u8],
    depth: usize,
    out: &mut Vec<u8>,
    remaining: &mut usize,
) -> Result<u8, CovenantDecodeError> {
    if depth > MAX_COVENANT_DEPTH {
        return Err(CovenantDecodeError::ExceededMaxDepth {
            max: MAX_COVENANT_DEPTH,
        });
    }
    if bytes.is_empty() {
        return Ok(0);
    }
    if bytes.len() > MAX_COVENANT_BYTES {
        return Err(CovenantDecodeError::ExceededMaxBytes);
    }

    let mut reader = bytes;
    let mut num_tokens = 0u8;
    while let Some(code) = reader.read_next_byte_code()? {
        *remaining = remaining.checked_sub(1).ok_or(CovenantDecodeError::ExceededMaxTokens {
            max: MAX_COVENANT_TOKENS,
        })?;
        num_tokens = num_tokens.saturating_add(1);

        if CovenantFilter::is_valid_code(code) {
            CovenantFilter::try_from_byte_code(code)?;
            out.push(code);
        } else if code == ARG_COVENANT {
            let len = read_canonical_len(&mut reader, MAX_COVENANT_ARG_SIZE)?;
            let (nested_bytes, rest) = reader.split_at_checked(len).ok_or(CovenantDecodeError::UnexpectedEof {
                expected: "covenant argument",
            })?;
            reader = rest;
            let mut nested = Vec::with_capacity(len);
            scan(nested_bytes, depth.saturating_add(1), &mut nested, remaining)?;
            out.push(ARG_COVENANT);
            out.write_varint(nested.len())?;
            out.extend_from_slice(&nested);
        } else if CovenantArg::is_valid_code(code) {
            let arg = CovenantArg::read_from_at_depth(&mut reader, code, depth)?;
            arg.write_to(out)?;
        } else {
            return Err(CovenantDecodeError::UnknownByteCode { code });
        }
    }
    Ok(num_tokens)
}
