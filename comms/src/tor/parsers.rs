// Copyright 2020, The Tari Project
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

use super::response::ResponseLine;
use nom::{
    bytes::complete::take_while1,
    character::{
        complete::{anychar, char as nom_char, digit1},
        is_alphanumeric,
    },
    combinator::map_res,
    error::ErrorKind,
};
use std::{borrow::Cow, fmt};

type NomErr<'a> = nom::Err<(&'a str, ErrorKind)>;

#[derive(Debug, Clone)]
pub struct ParseError(pub String);

impl From<NomErr<'_>> for ParseError {
    fn from(err: NomErr<'_>) -> Self {
        ParseError(err.to_string())
    }
}

impl std::error::Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "ParseError({})", self.0)
    }
}

pub fn response_line(line: &str) -> Result<ResponseLine<'_>, ParseError> {
    let parser = map_res(digit1, |code: &str| code.parse::<u16>());
    let (rest, code) = parser(line)?;
    let (rest, ch) = anychar(rest)?;
    if ch != ' ' && ch != '-' {
        return Err(ParseError(format!(
            "Unexpected end-of-response character '{}'. Expected ' ' or '-'.",
            ch
        )));
    }

    Ok(ResponseLine {
        has_more: ch == '-',
        code,
        value: rest.into(),
    })
}

pub fn key_value(line: &str) -> Result<(Cow<'_, str>, Cow<'_, str>), ParseError> {
    let (rest, identifier) = take_while1(|ch| is_alphanumeric(ch as u8))(line)?;
    let (rest, _) = nom_char('=')(rest)?;
    Ok((identifier.into(), rest.into()))
}
