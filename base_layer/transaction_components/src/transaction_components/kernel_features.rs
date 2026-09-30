// Copyright 2019. The Tari Project
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
use std::io;

use bitflags::bitflags;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

/// Options for a kernel's structure or use.
///
/// Encoded as a single `u8`. Every decoder (serde, borsh and the protobuf conversions) rejects bits that are not a
/// known flag, so a kernel decoded from any format only carries known features.
#[derive(Serialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelFeatures(u8);

bitflags! {
    impl KernelFeatures: u8 {
        /// Coinbase transaction
        const COINBASE_KERNEL = 1u8;
        /// Burned output transaction
        const BURN_KERNEL = 2u8;
    }
}

impl KernelFeatures {
    /// Creates a coinbase kernel flag
    pub fn create_coinbase() -> KernelFeatures {
        KernelFeatures::COINBASE_KERNEL
    }

    /// Creates a burned kernel flag
    pub fn create_burn() -> KernelFeatures {
        KernelFeatures::BURN_KERNEL
    }

    /// Does this feature include the burned flag?
    pub fn is_burned(&self) -> bool {
        self.contains(KernelFeatures::BURN_KERNEL)
    }

    /// Does this feature include the coinbase flag?
    pub fn is_coinbase(&self) -> bool {
        self.contains(KernelFeatures::COINBASE_KERNEL)
    }
}

impl Default for KernelFeatures {
    fn default() -> Self {
        KernelFeatures::empty()
    }
}

fn unknown_kernel_features_error(bits: u8) -> String {
    format!("Invalid or unrecognised kernel feature flags: {bits:#04x}")
}

impl<'de> Deserialize<'de> for KernelFeatures {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        // Mirrors the derived `Serialize` (a newtype struct around a `u8`) so the encoding is unchanged.
        #[derive(Deserialize)]
        #[serde(rename = "KernelFeatures")]
        struct Raw(u8);

        let Raw(bits) = Raw::deserialize(deserializer)?;
        KernelFeatures::from_bits(bits).ok_or_else(|| D::Error::custom(unknown_kernel_features_error(bits)))
    }
}

impl BorshDeserialize for KernelFeatures {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let bits = u8::deserialize_reader(reader)?;
        KernelFeatures::from_bits(bits)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, unknown_kernel_features_error(bits)))
    }
}

#[cfg(test)]
mod test {
    use super::KernelFeatures;

    const VALID: [KernelFeatures; 4] = [
        KernelFeatures::empty(),
        KernelFeatures::COINBASE_KERNEL,
        KernelFeatures::BURN_KERNEL,
        KernelFeatures::all(),
    ];
    const UNKNOWN: [u8; 3] = [0x04, 0x05, 0xFF];

    #[test]
    fn serde_json_rejects_unknown_bits_and_round_trips_known_flags() {
        for bits in UNKNOWN {
            let err = serde_json::from_str::<KernelFeatures>(&bits.to_string()).unwrap_err();
            assert!(err.to_string().contains("unrecognised kernel feature"), "{err}");
        }
        for features in VALID {
            let encoded = serde_json::to_string(&features).unwrap();
            assert_eq!(encoded, serde_json::to_string(&features.bits()).unwrap());
            assert_eq!(serde_json::from_str::<KernelFeatures>(&encoded).unwrap(), features);
        }
    }

    #[test]
    fn bincode_rejects_unknown_bits_and_round_trips_known_flags() {
        for bits in UNKNOWN {
            assert!(bincode::deserialize::<KernelFeatures>(&[bits]).is_err());
        }
        for features in VALID {
            let encoded = bincode::serialize(&features).unwrap();
            assert_eq!(encoded, bincode::serialize(&features.bits()).unwrap());
            assert_eq!(encoded, vec![features.bits()]);
            assert_eq!(bincode::deserialize::<KernelFeatures>(&encoded).unwrap(), features);
        }
    }

    #[test]
    fn borsh_rejects_unknown_bits_and_round_trips_known_flags() {
        for bits in UNKNOWN {
            let err = borsh::from_slice::<KernelFeatures>(&[bits]).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        }
        for features in VALID {
            let encoded = borsh::to_vec(&features).unwrap();
            assert_eq!(encoded, borsh::to_vec(&features.bits()).unwrap());
            assert_eq!(encoded, vec![features.bits()]);
            assert_eq!(borsh::from_slice::<KernelFeatures>(&encoded).unwrap(), features);
        }
    }

    #[test]
    fn every_decoder_accepts_exactly_the_known_bits() {
        for bits in 0..=u8::MAX {
            let known = KernelFeatures::from_bits(bits).is_some();
            assert_eq!(
                borsh::from_slice::<KernelFeatures>(&[bits]).is_ok(),
                known,
                "borsh {bits}"
            );
            assert_eq!(
                bincode::deserialize::<KernelFeatures>(&[bits]).is_ok(),
                known,
                "bincode {bits}"
            );
            assert_eq!(
                serde_json::from_str::<KernelFeatures>(&bits.to_string()).is_ok(),
                known,
                "serde_json {bits}"
            );
        }
    }

    #[test]
    fn test_all_possible_parses() {
        let x = super::KernelFeatures::from_bits(0);
        assert_eq!(x, Some(super::KernelFeatures::empty()));
        let x = super::KernelFeatures::from_bits(1);
        assert_eq!(x, Some(super::KernelFeatures::COINBASE_KERNEL));
        let x = super::KernelFeatures::from_bits(2);
        assert_eq!(x, Some(super::KernelFeatures::BURN_KERNEL));
        let x = super::KernelFeatures::from_bits(3);
        assert_eq!(
            x,
            Some(super::KernelFeatures::COINBASE_KERNEL | super::KernelFeatures::BURN_KERNEL)
        );
        let x = super::KernelFeatures::from_bits(4);
        assert_eq!(x, None);
        for i in 5..=u8::MAX {
            assert_eq!(None, super::KernelFeatures::from_bits(i));
        }
    }
}
