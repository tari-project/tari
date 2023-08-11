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

use derive_more::IntoIterator;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::multispace0,
    error::Error,
    multi::many0,
    sequence::{delimited, preceded},
    Err,
    IResult,
};

/// A parser that splits a line into parts
/// taking into account quotarion marks.
#[derive(IntoIterator)]
pub struct ParsedCommand<'a> {
    items: Vec<&'a str>,
}

impl<'a> ParsedCommand<'a> {
    /// Parse a string into a command.
    pub fn parse(line: &'a str) -> Result<Self, anyhow::Error> {
        parse(line).map_err(|err| anyhow::Error::msg(err.to_string()))
    }
}

/// Parses a string and drops the input if succeed.
fn parse(input: &str) -> Result<ParsedCommand<'_>, Err<Error<&str>>> {
    parse_command(input).map(|pair| pair.1)
}

/// Trims and parses command.
fn parse_command(input: &str) -> IResult<&str, ParsedCommand<'_>> {
    let input = input.trim();
    let (input, pairs) = parse_parameters(input)?;
    let command = ParsedCommand { items: pairs };
    Ok((input, command))
}

/// Parses many parameters delimited by a multispace.
fn parse_parameters(input: &str) -> IResult<&str, Vec<&str>> {
    many0(preceded(multispace0, parse_item))(input)
}

const PQ: &str = "\"";
const SQ: &str = "'";

/// Not space chars allowed for a paramters.
fn is_valid_char(c: char) -> bool {
    c != ' ' && c != '\t'
}

/// Validate parameter's chars.
fn valid_item(input: &str) -> IResult<&str, &str> {
    take_while1(is_valid_char)(input)
}

/// Parses a parameter that can be represented as:
/// - "parameter in double quotes"
/// - 'parameter in signle quotes'
/// - parameter-with-no-space
fn parse_item(input: &str) -> IResult<&str, &str> {
    alt((
        delimited(tag(PQ), take_until(PQ), tag(PQ)),
        delimited(tag(SQ), take_until(SQ), tag(SQ)),
        valid_item,
    ))(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser() {
        let items = parse("command").unwrap().items;
        assert_eq!(items, vec!["command"]);
        let items = parse("command with parameters").unwrap().items;
        assert_eq!(items, vec!["command", "with", "parameters"]);
        let items = parse("command 0.5 0.10").unwrap().items;
        assert_eq!(items, vec!["command", "0.5", "0.10"]);
        let items = parse("🇺🇦🕊️ 🛑⚔️").unwrap().items;
        assert_eq!(items, vec!["🇺🇦🕊️", "🛑⚔️"]);
        let items = parse("command extra,value check with:other;chars").unwrap().items;
        assert_eq!(items, vec!["command", "extra,value", "check", "with:other;chars"]);
        let items = parse("command with 'quoted long' \"parameters in\" \"a different \" format")
            .unwrap()
            .items;
        assert_eq!(items, vec![
            "command",
            "with",
            "quoted long",
            "parameters in",
            "a different ",
            "format"
        ]);
    }
}
