// Copyright 2021. The Tari Project
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
//
use std::{
    self,
    io::{self, BufRead, Read, Write},
    net::TcpStream,
};

use bufstream::BufStream;
use native_tls::{TlsConnector, TlsStream};

use crate::stratum::error::Error;

pub(crate) struct Stream {
    stream: Option<BufStream<TcpStream>>,
    tls_stream: Option<BufStream<TlsStream<TcpStream>>>,
}

impl Stream {
    pub fn new() -> Stream {
        Stream {
            stream: None,
            tls_stream: None,
        }
    }

    pub fn try_connect(&mut self, server_url: &str, tls: Option<bool>) -> Result<(), Error> {
        let conn = TcpStream::connect(server_url)?;
        if let Some(true) = tls {
            let connector = TlsConnector::new()?;
            //.map_err(|e| Error::Connection(format!(" {:?}", e)))?;
            let url_port: Vec<&str> = server_url.split(':').collect();
            let split_url: Vec<&str> = url_port[0].split('.').collect();
            let base_host = format!("{}.{}", split_url[split_url.len() - 2], split_url[split_url.len() - 1]);
            let mut stream = connector.connect(&base_host, conn)?;
            //.map_err(|e| Error::Connection(format!("Can't establish TLS connection: {:?}", e)))?;
            stream.get_mut().set_nonblocking(true)?;
            //.map_err(|e| Error::Connection(format!("Can't switch to nonblocking mode: {:?}", e)))?;
            self.tls_stream = Some(BufStream::new(stream));
        } else {
            conn.set_nonblocking(true)?;
            //.map_err(|e| Error::Connection(format!("Can't switch to nonblocking mode: {:?}", e)))?;
            self.stream = Some(BufStream::new(conn));
        }
        Ok(())
    }
}

impl Write for Stream {
    fn write(&mut self, b: &[u8]) -> Result<usize, std::io::Error> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().write(b)
        } else {
            self.stream.as_mut().unwrap().write(b)
        }
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().flush()
        } else {
            self.stream.as_mut().unwrap().flush()
        }
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().read(buf)
        } else {
            self.stream.as_mut().unwrap().read(buf)
        }
    }
}

impl BufRead for Stream {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().fill_buf()
        } else {
            self.stream.as_mut().unwrap().fill_buf()
        }
    }

    fn consume(&mut self, amt: usize) {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().consume(amt)
        } else {
            self.stream.as_mut().unwrap().consume(amt)
        }
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<usize> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().read_until(byte, buf)
        } else {
            self.stream.as_mut().unwrap().read_until(byte, buf)
        }
    }

    fn read_line(&mut self, string: &mut String) -> io::Result<usize> {
        if self.tls_stream.is_some() {
            self.tls_stream.as_mut().unwrap().read_line(string)
        } else {
            self.stream.as_mut().unwrap().read_line(string)
        }
    }
}
