//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::io;

use crate::covenants::token::CovenantToken;

pub struct CovenantTokenEncoder<'a> {
    tokens: &'a [CovenantToken],
}

impl<'a> CovenantTokenEncoder<'a> {
    pub fn new(tokens: &'a [CovenantToken]) -> Self {
        Self { tokens }
    }

    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        for token in self.tokens {
            token.write_to(writer)?;
        }
        Ok(())
    }
}

pub(super) trait CovenentWriteExt: io::Write {
    fn write_u8_fixed(&mut self, v: u8) -> Result<usize, io::Error>;
}

impl<W: io::Write> CovenentWriteExt for W {
    fn write_u8_fixed(&mut self, v: u8) -> Result<usize, io::Error> {
        self.write_all(&[v])?;
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use tari_common_types::types::FixedHash;

    use super::*;
    use crate::{
        covenant,
        covenants::{
            byte_codes::{ARG_HASH, ARG_OUTPUT_FIELD, FILTER_AND, FILTER_FIELD_EQ, FILTER_IDENTITY, FILTER_OR},
            OutputField,
        },
    };

    #[test]
    fn it_encodes_empty_tokens() {
        let encoder = CovenantTokenEncoder::new(&[]);
        let mut buf = Vec::<u8>::new();
        encoder.write_to(&mut buf).unwrap();
        assert_eq!(buf, [] as [u8; 0]);
    }

    #[test]
    fn it_encodes_tokens_correctly() -> Result<(), Box<dyn std::error::Error>> {
        let covenant = covenant!(and(identity(), or(identity()))).unwrap();
        let encoder = CovenantTokenEncoder::new(covenant.tokens());
        let mut buf = Vec::<u8>::new();
        encoder.write_to(&mut buf).unwrap();
        assert_eq!(buf, [FILTER_AND, FILTER_IDENTITY, FILTER_OR, FILTER_IDENTITY]);
        Ok(())
    }

    #[test]
    fn it_encodes_args_correctly() -> Result<(), Box<dyn std::error::Error>> {
        let dummy = FixedHash::zero();
        let covenant = covenant!(field_eq(@field::features, @hash(dummy))).unwrap();
        let encoder = CovenantTokenEncoder::new(covenant.tokens());
        let mut buf = Vec::<u8>::new();
        encoder.write_to(&mut buf).unwrap();
        assert_eq!(buf[..4], [
            FILTER_FIELD_EQ,
            ARG_OUTPUT_FIELD,
            OutputField::Features.as_byte(),
            ARG_HASH
        ]);
        assert_eq!(buf[4..], [0u8; 32]);
        Ok(())
    }

    mod covenant_write_ext {
        use super::*;

        #[test]
        fn it_writes_a_single_byte() {
            let mut buf = Vec::new();
            buf.write_u8_fixed(123u8).unwrap();
            assert_eq!(buf, vec![123u8]);
        }
    }
}
