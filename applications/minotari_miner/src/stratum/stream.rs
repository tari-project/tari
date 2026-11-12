//  Copyright 2024. The Tari Project
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
use std::{
    io::{self, BufRead, Read, Write},
    net::TcpStream,
};

use bufstream::BufStream;
use native_tls::{TlsConnector, TlsStream};

use crate::stratum::error::Error;

pub(crate) enum Stream {
    Stream(BufStream<TcpStream>),
    TlsStream(Box<BufStream<TlsStream<TcpStream>>>),
}

impl Stream {
    pub fn try_connect(server_url: &str, tls: Option<bool>) -> Result<Self, Error> {
        let conn = TcpStream::connect(server_url)?;
        if let Some(true) = tls {
            let connector = TlsConnector::new()?;
            let host = extract_tls_host(server_url)?;
            let mut stream = connector.connect(host, conn).map_err(Box::new)?;
            stream.get_mut().set_nonblocking(true)?;
            Ok(Self::TlsStream(Box::from(BufStream::new(stream))))
        } else {
            conn.set_nonblocking(true)?;
            Ok(Self::Stream(BufStream::new(conn)))
        }
    }

    fn reader(&mut self) -> &mut dyn Read {
        match self {
            Self::TlsStream(tls_stream) => tls_stream,
            Self::Stream(stream) => stream,
        }
    }

    fn writer(&mut self) -> &mut dyn Write {
        match self {
            Self::TlsStream(tls_stream) => tls_stream,
            Self::Stream(stream) => stream,
        }
    }

    fn buf_reader(&mut self) -> &mut dyn BufRead {
        match self {
            Self::TlsStream(tls_stream) => tls_stream,
            Self::Stream(stream) => stream,
        }
    }
}

impl Write for Stream {
    fn write(&mut self, b: &[u8]) -> Result<usize, io::Error> {
        self.writer().write(b)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.writer().flush()
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader().read(buf)
    }
}

impl BufRead for Stream {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.buf_reader().fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.buf_reader().consume(amt)
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.buf_reader().read_until(byte, buf)
    }

    fn read_line(&mut self, string: &mut String) -> io::Result<usize> {
        self.buf_reader().read_line(string)
    }
}

fn extract_tls_host(server_url: &str) -> Result<&str, Error> {
    let trimmed = server_url.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        let (host, port_part) = rest
            .split_once("]:")
            .ok_or_else(|| Error::MissingData("No port".to_string()))?;
        if host.is_empty() || port_part.is_empty() {
            return Err(Error::MissingData("No url".to_string()));
        }
        return Ok(host);
    }
    let (host, port_part) = trimmed
        .rsplit_once(':')
        .ok_or_else(|| Error::MissingData("No port".to_string()))?;
    if host.is_empty() || port_part.is_empty() {
        return Err(Error::MissingData("No url".to_string()));
    }
    Ok(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tls_host_preserves_full_hostname() {
        assert_eq!(extract_tls_host("pool.tari.com:3333").unwrap(), "pool.tari.com");
        assert_eq!(
            extract_tls_host("eu.stratum.tari.co.uk:443").unwrap(),
            "eu.stratum.tari.co.uk"
        );
        assert_eq!(extract_tls_host("localhost:3333").unwrap(), "localhost");
        assert_eq!(extract_tls_host("127.0.0.1:3333").unwrap(), "127.0.0.1");
        assert_eq!(extract_tls_host("[::1]:3333").unwrap(), "::1");
        assert!(extract_tls_host("pool.tari.com").is_err());
        assert!(extract_tls_host(":3333").is_err());
        assert!(extract_tls_host("pool.tari.com:").is_err());
        assert!(extract_tls_host("[]:3333").is_err());
    }
}
