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

    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = 0;
        for token in self.tokens {
            written += token.write_to(writer)?;
        }
        Ok(written)
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
