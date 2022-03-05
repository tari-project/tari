use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take_until, take_while1},
    character::complete::{char, multispace0, multispace1},
    combinator::rest,
    error::Error,
    multi::many0,
    sequence::{delimited, preceded, separated_pair},
    Err,
    IResult,
};

pub struct ParsedCommand<'a> {
    items: Vec<&'a str>,
}

impl<'a> ParsedCommand<'a> {
    pub fn parse(input: &'a str) -> Result<ParsedCommand<'a>, Err<Error<&str>>> {
        parse_command(input).map(|pair| pair.1)
    }
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
