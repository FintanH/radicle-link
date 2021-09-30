// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Neg,
};

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, digit1},
    combinator::{eof, map, value},
    multi::separated_list1,
    sequence::{delimited, preceded, separated_pair, terminated},
};

use crate::{
    json::{Cjson as _, Value},
    Cstring,
};

mod string;
use string::parse_string;

pub fn json(i: &str) -> nom::IResult<&str, Value> {
    terminated(alt((string, number, object, array, boolean, null)), eof)(i)
}

fn object(i: &str) -> nom::IResult<&str, Value> {
    map(alt((empty_object, full_object)), |o| Value::Object(o))(i)
}

fn empty_object(i: &str) -> nom::IResult<&str, BTreeMap<Cstring, Value>> {
    map(tag("{}"), |_| BTreeMap::new())(i)
}

fn full_object(i: &str) -> nom::IResult<&str, BTreeMap<Cstring, Value>> {
    map(delimited(char('{'), members, char('}')), |ms| {
        ms.into_iter().collect()
    })(i)
}

fn members(i: &str) -> nom::IResult<&str, Vec<(Cstring, Value)>> {
    separated_list1(char(','), pair)(i)
}

fn pair(i: &str) -> nom::IResult<&str, (Cstring, Value)> {
    separated_pair(cstring, char(':'), json)(i)
}

fn array(i: &str) -> nom::IResult<&str, Value> {
    map(alt((empty_array, full_array)), |a| Value::Array(a))(i)
}

fn empty_array(i: &str) -> nom::IResult<&str, BTreeSet<Value>> {
    map(tag("[]"), |_| BTreeSet::new())(i)
}

fn full_array(i: &str) -> nom::IResult<&str, BTreeSet<Value>> {
    map(separated_list1(char(','), json), |vs| {
        vs.into_iter().collect()
    })(i)
}

fn null(i: &str) -> nom::IResult<&str, Value> {
    value(Value::Null, tag("null"))(i)
}

fn boolean(i: &str) -> nom::IResult<&str, Value> {
    alt((
        value(Value::Bool(true), tag("true")),
        value(Value::Bool(false), tag("false")),
    ))(i)
}

fn string(i: &str) -> nom::IResult<&str, Value> {
    map(cstring, |s| s.into_cjson())(i)
}

fn cstring(i: &str) -> nom::IResult<&str, Cstring> {
    map(parse_string, |s| Cstring::from(s))(i)
}

fn number(i: &str) -> nom::IResult<&str, Value> {
    alt((signed, unsigned))(i)
}

fn signed(i: &str) -> nom::IResult<&str, Value> {
    preceded(
        minus,
        map(digit1, |digits: &str| {
            digits.parse::<i64>().unwrap().neg().into_cjson()
        }),
    )(i)
}

fn unsigned(i: &str) -> nom::IResult<&str, Value> {
    map(digit1, |digits: &str| {
        digits.parse::<u64>().unwrap().into_cjson()
    })(i)
}

fn minus(i: &str) -> nom::IResult<&str, char> {
    char('-')(i)
}
