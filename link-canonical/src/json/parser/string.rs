// Copyright (c) 2014-2019 Geoffroy Couprie.
// SPDX-License-Identifier: MIT

use nom::{
    branch::alt,
    bytes::streaming::{tag, take_while1},
    character::streaming::{char, hex_digit0, multispace1},
    combinator::{map, recognize, value},
    error::{FromExternalError, ParseError},
    multi::fold_many0,
    sequence::{delimited, pair, preceded},
    IResult,
};
use unicode_normalization::UnicodeNormalization;

pub static LUT: [(&str, &str); 34] = [
    ("\\x00", "\\u0000"),
    ("\\x01", "\\u0001"),
    ("\\x02", "\\u0002"),
    ("\\x03", "\\u0003"),
    ("\\x04", "\\u0004"),
    ("\\x05", "\\u0005"),
    ("\\x06", "\\u0006"),
    ("\\x07", "\\u0007"),
    ("\\x08", "\\b"),
    ("\\x09", "\\t"),
    ("\\x0a", "\\n"),
    ("\\x0b", "\\u000b"),
    ("\\x0c", "\\f"),
    ("\\x0d", "\\r"),
    ("\\x0e", "\\u000e"),
    ("\\x0f", "\\u000f"),
    ("\\x10", "\\u0010"),
    ("\\x11", "\\u0011"),
    ("\\x12", "\\u0012"),
    ("\\x13", "\\u0013"),
    ("\\x14", "\\u0014"),
    ("\\x15", "\\u0015"),
    ("\\x16", "\\u0016"),
    ("\\x17", "\\u0017"),
    ("\\x18", "\\u0018"),
    ("\\x19", "\\u0019"),
    ("\\x1a", "\\u001a"),
    ("\\x1b", "\\u001b"),
    ("\\x1c", "\\u001c"),
    ("\\x1d", "\\u001d"),
    ("\\x1e", "\\u001e"),
    ("\\x1f", "\\u001f"),
    ("\\x22", "\\\""),
    ("\\x5c", "\\\\"),
];

fn is_nonescaped_string_char(c: char) -> bool {
    let cv = c as u32;
    (cv >= 0x20) && (cv != 0x22) && (cv != 0x5C)
}

fn control_code<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str>,
{
    map(
        recognize(pair(
            tag("\\x"),
            map(recognize(pair(hex_digit0, hex_digit0)), |hex: &str| {
                hex.to_ascii_lowercase()
            }),
        )),
        |code: &str| {
            LUT.iter()
                .cloned()
                .find_map(|(k, v): (&str, &str)| -> Option<&str> {
                    if k == code {
                        Some(v)
                    } else {
                        None
                    }
                })
                .unwrap_or(code)
        },
    )(input)
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
    alt((
        control_code,
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

fn parse_fragment<'a, E>(input: &'a str) -> IResult<&'a str, StringFragment<'a>, E>
where
    E: ParseError<&'a str> + FromExternalError<&'a str, std::num::ParseIntError>,
{
    alt((
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
    let build_string = fold_many0(
        parse_fragment,
        String::new,
        |mut string, fragment| {
            match fragment {
                StringFragment::Literal(s) | StringFragment::EscapedChar(s) => string.push_str(s),
                StringFragment::EscapedWS => {},
            }
            string
        },
    );

    // Normalize Form C the resulting string
    map(delimited(char('"'), build_string, char('"')), |s| {
        s.nfc().fold(String::new(), |mut acc, ch| {
            let mut buf = [0; 4];
            let s = ch.encode_utf8(&mut buf);
            acc.push_str(s);
            acc
        })
    })(input)
}

