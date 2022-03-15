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
    many0(preceded(multispace0, parse_item))(input)
}

const PQ: &str = "\"";
const SQ: &str = "'";

fn is_valid_char(c: char) -> bool {
    c != ' ' && c != '\t'
}

fn valid_item(input: &str) -> IResult<&str, &str> {
    take_while1(is_valid_char)(input)
}

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
        // TODO: Support emojis
    }
}
