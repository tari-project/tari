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

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Options for a kernel's structure or use.
#[derive(Serialize, Deserialize, BorshSerialize, BorshDeserialize, Clone, Copy, Debug, Eq, PartialEq)]
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

#[cfg(test)]
mod test {
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
