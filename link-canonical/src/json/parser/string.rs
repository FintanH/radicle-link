// Copyright (c) 2014-2019 Geoffroy Couprie.
// SPDX-License-Identifier: MIT

use nom::{
    branch::alt,
    bytes::streaming::{tag, take_while1},
    character::streaming::{char, multispace1},
    combinator::{map, recognize, value},
    error::{FromExternalError, ParseError},
    multi::fold_many0,
    sequence::{delimited, pair, preceded},
    IResult,
};

fn is_nonescaped_string_char(c: char) -> bool {
    let cv = c as u32;
    (cv >= 0x20) && (cv != 0x22) && (cv != 0x5C)
}

// One or more unescaped text characters
fn nonescaped_string<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str>,
{
    take_while1(is_nonescaped_string_char)(input)
}

fn escape_code<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str>,
{
    recognize(pair(
        tag("\\"),
        alt((
            tag("\""),
            tag("\\"),
            tag("/"),
            tag("b"),
            tag("f"),
            tag("n"),
            tag("r"),
            tag("t"),
            tag("u"),
        )),
    ))(input)
}

/// Parse a backslash, followed by any amount of whitespace. This is used later
/// to discard any escaped whitespace.
fn parse_escaped_whitespace<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    preceded(char('\\'), multispace1)(input)
}

/// A string fragment contains a fragment of a string being parsed: either
/// a non-empty Literal (a series of non-escaped characters), a single
/// parsed escaped character, or a block of escaped whitespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringFragment<'a> {
    Literal(&'a str),
    EscapedChar(&'a str),
    EscapedWS,
}

/// Combine parse_literal, parse_escaped_whitespace, and parse_escaped_char
/// into a StringFragment.
fn parse_fragment<'a, E>(input: &'a str) -> IResult<&'a str, StringFragment<'a>, E>
where
    E: ParseError<&'a str> + FromExternalError<&'a str, std::num::ParseIntError>,
{
    alt((
        // The `map` combinator runs a parser, then applies a function to the output
        // of that parser.
        map(nonescaped_string, StringFragment::Literal),
        map(escape_code, StringFragment::EscapedChar),
        value(StringFragment::EscapedWS, parse_escaped_whitespace),
    ))(input)
}

/// Parse a string. Use a loop of parse_fragment and push all of the fragments
/// into an output string.
pub fn parse<'a, E>(input: &'a str) -> IResult<&'a str, String, E>
where
    E: ParseError<&'a str> + FromExternalError<&'a str, std::num::ParseIntError>,
{
    // fold_many0 is the equivalent of iterator::fold. It runs a parser in a loop,
    // and for each output value, calls a folding function on each output value.
    let build_string = fold_many0(
        // Our parser function– parses a single string fragment
        parse_fragment,
        // Our init value, an empty string
        String::new,
        // Our folding function. For each fragment, append the fragment to the
        // string.
        |mut string, fragment| {
            match fragment {
                StringFragment::Literal(s) => string.push_str(s),
                StringFragment::EscapedChar(c) => string.push_str(c),
                StringFragment::EscapedWS => {},
            }
            string
        },
    );

    // Finally, parse the string. Note that, if `build_string` could accept a raw
    // " character, the closing delimiter " would never match. When using
    // `delimited` with a looping parser (like fold_many0), be sure that the
    // loop won't accidentally match your closing delimiter!
    delimited(char('"'), build_string, char('"'))(input)
}
