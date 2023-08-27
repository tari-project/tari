// Copyright 2020. The Taiji Project
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

use thiserror::Error;

/// Calculates a checksum using the [DammSum](https://github.com/cypherstack/dammsum) algorithm.
///
/// This approach uses a dictionary whose size must be `2^k` for some `k > 0`.
/// The algorithm accepts an array of arbitrary size, each of whose elements are integers in the range `[0, 2^k)`.
/// The checksum is a single element also within this range.
/// DammSum detects all single transpositions and substitutions.
///
/// Note that for this implementation, we add the additional restriction that `k == 8`.
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

// Fixed for a dictionary size of `2^8 == 256`
const COEFFICIENTS: [u8; 3] = [4, 3, 1];

/// Compute the DammSum checksum for an array, each of whose elements are in the range `[0, 2^8)`
pub fn compute_checksum(data: &Vec<u8>) -> u8 {
    let mut mask = 1u8;

    // Compute the bitmask (if possible)
    for bit in COEFFICIENTS {
        mask += 1u8 << bit;
    }

    // Perform the Damm algorithm
    let mut result = 0u8;

    for digit in data {
        result ^= *digit; // add
        let overflow = (result & (1 << 7)) != 0;
        result <<= 1; // double
        if overflow {
            // reduce
            result ^= mask;
        }
    }

    result
}

/// Determine whether the array ends with a valid checksum
pub fn validate_checksum(data: &Vec<u8>) -> Result<(), ChecksumError> {
    // Empty data is not allowed, nor data only consisting of a checksum
    if data.len() < 2 {
        return Err(ChecksumError::InputDataTooShort);
    }

    // It's sufficient to check the entire array against a zero checksum
    match compute_checksum(data) {
        0u8 => Ok(()),
        _ => Err(ChecksumError::InvalidChecksum),
    }
}

#[cfg(test)]
mod test {
    use rand::Rng;

    use crate::dammsum::{compute_checksum, validate_checksum, ChecksumError};

    #[test]
    /// Check that valid checksums validate
    fn checksum_validate() {
        const SIZE: usize = 33;

        // Generate random data
        let mut rng = rand::thread_rng();
        let mut data: Vec<u8> = (0..SIZE).map(|_| rng.gen::<u8>()).collect();

        // Compute and append the checksum
        data.push(compute_checksum(&data));

        // Validate
        assert!(validate_checksum(&data).is_ok());
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
}
