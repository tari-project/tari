use derive_more::IntoIterator;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::multispace1,
    error::Error,
    multi::many0,
    sequence::{delimited, preceded},
    Err,
    IResult,
};

#[derive(IntoIterator)]
pub struct ParsedCommand<'a> {
    items: Vec<&'a str>,
}

impl<'a> ParsedCommand<'a> {
    pub fn parse(line: &'a str) -> Result<Self, anyhow::Error> {
        parse(line).map_err(|err| anyhow::Error::msg(err.to_string()))
    }
}

fn parse(input: &str) -> Result<ParsedCommand<'_>, Err<Error<&str>>> {
    parse_command(input).map(|pair| pair.1)
}

fn parse_command(input: &str) -> IResult<&str, ParsedCommand<'_>> {
    let input = input.trim();

    let (input, pairs) = parse_parameters(input)?;

    let command = ParsedCommand { items: pairs };

    Ok((input, command))
}

fn parse_parameters(input: &str) -> IResult<&str, Vec<&str>> {
    many0(preceded(multispace1, parse_pair))(input)
}

const PQ: &str = "\"";
const SQ: &str = "'";

fn parse_pair(input: &str) -> IResult<&str, &str> {
    alt((
        delimited(tag(PQ), take_until(PQ), tag(PQ)),
        delimited(tag(SQ), take_until(SQ), tag(SQ)),
    ))(input)
}
