// Copyright 2020. The Tari Project
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

use once_cell::sync::Lazy;
use thiserror::Error;

/// Calculates a checksum using the [DammSum](https://github.com/cypherstack/dammsum) algorithm.
///
/// This approach uses a dictionary whose size must be `2^k` for some `k > 0`.
/// The algorithm accepts a slice of arbitrary size, each of whose elements are integers in the range `[0, 2^k)`.
/// The checksum is a single element also within this range.
/// DammSum detects all single transpositions and substitutions.
///
/// Note that for this implementation, we add the additional restriction that `k == 8` to handle byte slices.
/// This is only because DammSum requires us to provide the coefficients for a certain type of polynomial, and
/// because it's unlikely for the alphabet size to change for this use case.
/// See the linked repository for more information, or if you need a different dictionary size.

#[derive(Debug, Error, PartialEq)]
pub enum ChecksumError {
    #[error("Input data is too short")]
    InputDataTooShort,
    #[error("Invalid checksum")]
    InvalidChecksum,
}

/// The number of bytes used for the checksum
/// This is included for applications that need to know it for encodings
pub const CHECKSUM_BYTES: usize = 1;

// Set up the mask, fixed for a dictionary size of `2^8 == 256`
// This can fail on invalid coefficients, which will cause a panic
// To ensure this doesn't happen in production, it is directly tested
const COEFFICIENTS: [u8; 3] = [4, 3, 1];
static MASK: Lazy<u8> = Lazy::new(|| {
    let mut mask = 1u8;

    for bit in COEFFICIENTS {
        let shift = 1u8.checked_shl(u32::from(bit)).unwrap();
        mask = mask.checked_add(shift).unwrap();
    }

    mask
});

/// Compute the DammSum checksum for a byte slice
pub fn compute_checksum(data: &[u8]) -> u8 {
    // Perform the Damm algorithm
    let mut result = 0u8;

    for digit in data {
        result ^= *digit; // add
        let overflow = (result & (1 << 7)) != 0;
        result <<= 1; // double
        if overflow {
            // reduce
            result ^= *MASK;
        }
    }

    result
}

/// Determine whether a byte slice ends with a valid checksum
/// If it is valid, returns the underlying data slice (without the checksum)
pub fn validate_checksum(data: &[u8]) -> Result<&[u8], ChecksumError> {
    // Empty data is not allowed, nor data only consisting of a checksum
    if data.len() < 2 {
        return Err(ChecksumError::InputDataTooShort);
    }

    // It's sufficient to check the entire slice against a zero checksum
    match compute_checksum(data) {
        0u8 => Ok(&data[..data.len() - 1]),
        _ => Err(ChecksumError::InvalidChecksum),
    }
}

#[cfg(test)]
mod test {
    use rand::Rng;

    use crate::dammsum::*;

    #[test]
    /// Check that mask initialization doesn't panic
    fn no_mask_panic() {
        let _mask = *MASK;
    }

    #[test]
    /// Check that valid checksums validate
    fn checksum_validate() {
        const SIZE: usize = 33;

        // Generate random data
        let mut rng = rand::thread_rng();
        let data: Vec<u8> = (0..SIZE).map(|_| rng.gen::<u8>()).collect();

        // Compute and append the checksum
        let mut data_with_checksum = data.clone();
        data_with_checksum.push(compute_checksum(&data));

        // Validate and ensure we get the same data back
        assert_eq!(validate_checksum(&data_with_checksum).unwrap(), data);
    }

    #[test]
    /// Sanity check against memory-specific checksums
    fn identical_checksum() {
        const SIZE: usize = 33;

        // Generate identical random data
        let mut rng = rand::thread_rng();
        let data_0: Vec<u8> = (0..SIZE).map(|_| rng.gen::<u8>()).collect();
        let check_0 = compute_checksum(&data_0);

        let data_1 = data_0;
        let check_1 = compute_checksum(&data_1);

        // They should be equal
        assert_eq!(check_0, check_1);
    }

    #[test]
    /// Sanity check for known distinct checksums
    fn distinct_checksum() {
        // Fix two inputs that must have a unique checksum
        let data_0 = vec![0u8];
        let data_1 = vec![1u8];

        // Compute the checksums
        let check_0 = compute_checksum(&data_0);
        let check_1 = compute_checksum(&data_1);

        // They should be distinct
        assert!(check_0 != check_1);
    }

    #[test]
    /// Test validation failure modes
    fn failure_modes_validate() {
        // Empty input data
        let mut data: Vec<u8> = vec![];
        assert_eq!(validate_checksum(&data), Err(ChecksumError::InputDataTooShort));

        // Input data is only a checksum
        data = vec![0u8];
        assert_eq!(validate_checksum(&data), Err(ChecksumError::InputDataTooShort));
    }

    #[test]
    /// Check that all single subtitutions are detected
    fn substitutions() {
        const SIZE: usize = 33;

        // Generate random data
        let mut rng = rand::thread_rng();
        let mut data: Vec<u8> = (0..SIZE).map(|_| rng.gen::<u8>()).collect();

        // Compute the checksum
        data.push(compute_checksum(&data));

        // Validate
        assert!(validate_checksum(&data).is_ok());

        // Check all substitutions in all positions
        for j in 0..data.len() {
            let mut data_ = data.clone();
            for i in 0..=u8::MAX {
                if data[j] == i {
                    continue;
                }
                data_[j] = i;

                assert_eq!(validate_checksum(&data_), Err(ChecksumError::InvalidChecksum));
            }
        }
    }

    #[test]
    /// Check that all single transpositions are detected
    fn transpositions() {
        const SIZE: usize = 33;

        // Generate random data
        let mut rng = rand::thread_rng();
        let mut data: Vec<u8> = (0..SIZE).map(|_| rng.gen::<u8>()).collect();

        // Compute the checksum
        data.push(compute_checksum(&data));

        // Validate
        assert!(validate_checksum(&data).is_ok());

        // Check all transpositions
        for j in 0..(data.len() - 1) {
            if data[j] == data[j + 1] {
                continue;
            }

            let mut data_ = data.clone();
            data_.swap(j, j + 1);

            assert_eq!(validate_checksum(&data_), Err(ChecksumError::InvalidChecksum));
        }
    }

    #[test]
    fn known_checksum() {
        const SIZE: usize = 33;

        // We know what the checksum for all-zero data must be
        assert_eq!(compute_checksum(&[0u8; SIZE]), 0u8);
    }
}
