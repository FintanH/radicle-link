// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    str::FromStr,
};

use nom::Finish as _;

use crate::{Canonical, Cstring};

mod parser;
mod ser;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Object(BTreeMap<Cstring, Value>),
    Array(BTreeSet<Value>),
    String(Cstring),
    Number(Number),
    Bool(bool),
    Null,
}

impl Canonical for Value {
    type Error = Infallible;

    fn canonical_form(&self) -> Result<Vec<u8>, Self::Error> {
        Ok(self.to_bytes())
    }
}

impl FromStr for Value {
    type Err = nom::error::Error<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parser::json(s).finish() {
            Ok((_remaining, value)) => Ok(value),
            Err(nom::error::Error { input, code }) => Err(nom::error::Error {
                input: input.to_string(),
                code,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Number {
    U64(u64),
    I64(i64),
}

impl Canonical for Number {
    type Error = Infallible;

    fn canonical_form(&self) -> Result<Vec<u8>, Self::Error> {
        Ok(self.to_bytes())
    }
}

pub trait Cjson {
    fn into_cjson(self) -> Value;
}

// Object

impl<T: Cjson> Cjson for BTreeMap<Cstring, T> {
    fn into_cjson(self) -> Value {
        into_object(self.into_iter())
    }
}

// Array

impl<T: Cjson + Ord> Cjson for BTreeSet<T> {
    fn into_cjson(self) -> Value {
        into_array(self.into_iter())
    }
}

// Option

impl<T: Cjson> Cjson for Option<T> {
    fn into_cjson(self) -> Value {
        match self {
            None => Value::Null,
            Some(t) => t.into_cjson(),
        }
    }
}

// Strings

impl Cjson for Cstring {
    fn into_cjson(self) -> Value {
        Value::String(self)
    }
}

// Numbers

impl Cjson for u64 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::U64(self))
    }
}

impl Cjson for u32 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::U64(self as u64))
    }
}

impl Cjson for u16 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::U64(self as u64))
    }
}

impl Cjson for u8 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::U64(self as u64))
    }
}

impl Cjson for i64 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::I64(self))
    }
}

impl Cjson for i32 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::I64(self as i64))
    }
}

impl Cjson for i16 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::I64(self as i64))
    }
}

impl Cjson for i8 {
    fn into_cjson(self) -> Value {
        Value::Number(Number::I64(self as i64))
    }
}

// Bool

impl Cjson for bool {
    fn into_cjson(self) -> Value {
        Value::Bool(self)
    }
}

// Iterator helpers

fn into_array<I, T>(it: I) -> Value
where
    I: Iterator<Item = T>,
    T: Ord + Cjson,
{
    Value::Array(it.map(Cjson::into_cjson).collect())
}

fn into_object<I, T>(it: I) -> Value
where
    I: Iterator<Item = (Cstring, T)>,
    T: Cjson,
{
    Value::Object(
        it.map(|(key, value)| (key, Cjson::into_cjson(value)))
            .collect(),
    )
}
