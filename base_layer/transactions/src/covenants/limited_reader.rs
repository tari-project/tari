//  Copyright 2022, The Tari Project
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

use std::{io, io::Read};

pub struct LimitedBytesReader<R> {
    byte_limit: usize,
    num_read: usize,
    inner: R,
}

impl<R: Read> LimitedBytesReader<R> {
    pub fn new(byte_limit: usize, reader: R) -> Self {
        Self {
            byte_limit,
            num_read: 0,
            inner: reader,
        }
    }
}
impl<R: Read> Read for LimitedBytesReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.num_read += read;
        if self.num_read > self.byte_limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Read more bytes than the maximum ({})", self.byte_limit),
            ));
        }
        Ok(read)
    }
}

#[cfg(test)]
mod test {
    use std::io::Read;

    use super::*;

    #[test]
    fn read_test() {
        // read should work fine in the case of a buffer whose length is within byte_limit
        let inner: &[u8] = &[0u8, 1u8, 2u8, 3u8, 4u8];
        let mut reader = LimitedBytesReader::new(3, inner);
        let mut buf = [0u8; 3];
        let output = reader.read(&mut buf).unwrap();
        assert_eq!(output, buf.len());

        // in case of buffer with length strictly bigger than reader byte_limit, the code should throw an error
        let mut new_buf = [0u8; 4];
        let output = reader.read(&mut new_buf);
        assert!(output.is_err());
    }
}
