// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use link_canonical::{
    json::{Array, Map, ToCjson, Value},
    Canonical,
    Cstring,
};

#[derive(ToCjson)]
#[cjson(rename_all = "camelCase")]
struct Foo {
    x_foo: u64,
    y_foo: Option<Cstring>,
}

#[derive(ToCjson)]
struct Bar(bool, bool);

#[derive(ToCjson)]
struct Baz;

#[derive(ToCjson)]
enum E {
    W { a: u32, b: i32 },
    X(u32, i32),
    Y(i32),
    Z,
}

fn roundtrip(s: &str) -> Result<(), String> {
    let val = s.parse::<Value>()?;
    assert_eq!(val.canonical_form().unwrap(), s.as_bytes());
    Ok(())
}

fn encode_string(s: &str) -> Result<String, String> {
    let bs = s.parse::<Value>()?.canonical_form().unwrap();
    Ok(std::str::from_utf8(&bs).unwrap().to_string())
}

#[test]
fn securesystemslib_asserts() -> Result<(), String> {
    roundtrip("[1,2,3]")?;
    roundtrip("[]")?;
    roundtrip("{}")?;
    roundtrip(r#"{"A":[99]}"#)?;
    roundtrip(r#"{"A":true}"#)?;
    roundtrip(r#"{"B":false}"#)?;
    roundtrip(r#"{"x":3,"y":2}"#)?;
    roundtrip(r#"{"x":3,"y":null}"#)?;

    // Test conditions for invalid arguments.
    assert!(roundtrip("8.0").is_err());
    assert!(roundtrip(r#"{"x": 8.0}"#).is_err());

    Ok(())
}

#[test]
fn ascii_control_characters() -> Result<(), String> {
    assert_eq!(&encode_string(r#""\x00""#)?, r#""\u0000""#);
    assert_eq!(&encode_string(r#""\x01""#)?, r#""\u0001""#);
    assert_eq!(&encode_string(r#""\x02""#)?, r#""\u0002""#);
    assert_eq!(&encode_string(r#""\x03""#)?, r#""\u0003""#);
    assert_eq!(&encode_string(r#""\x04""#)?, r#""\u0004""#);
    assert_eq!(&encode_string(r#""\x05""#)?, r#""\u0005""#);
    assert_eq!(&encode_string(r#""\x06""#)?, r#""\u0006""#);
    assert_eq!(&encode_string(r#""\x07""#)?, r#""\u0007""#);
    assert_eq!(&encode_string(r#""\x08""#)?, r#""\b""#);
    assert_eq!(&encode_string(r#""\x09""#)?, r#""\t""#);
    assert_eq!(&encode_string(r#""\x0a""#)?, r#""\n""#);
    assert_eq!(&encode_string(r#""\x0b""#)?, r#""\u000b""#);
    assert_eq!(&encode_string(r#""\x0c""#)?, r#""\f""#);
    assert_eq!(&encode_string(r#""\x0d""#)?, r#""\r""#);
    assert_eq!(&encode_string(r#""\x0e""#)?, r#""\u000e""#);
    assert_eq!(&encode_string(r#""\x0f""#)?, r#""\u000f""#);
    assert_eq!(&encode_string(r#""\x10""#)?, r#""\u0010""#);
    assert_eq!(&encode_string(r#""\x11""#)?, r#""\u0011""#);
    assert_eq!(&encode_string(r#""\x12""#)?, r#""\u0012""#);
    assert_eq!(&encode_string(r#""\x13""#)?, r#""\u0013""#);
    assert_eq!(&encode_string(r#""\x14""#)?, r#""\u0014""#);
    assert_eq!(&encode_string(r#""\x15""#)?, r#""\u0015""#);
    assert_eq!(&encode_string(r#""\x16""#)?, r#""\u0016""#);
    assert_eq!(&encode_string(r#""\x17""#)?, r#""\u0017""#);
    assert_eq!(&encode_string(r#""\x18""#)?, r#""\u0018""#);
    assert_eq!(&encode_string(r#""\x19""#)?, r#""\u0019""#);
    assert_eq!(&encode_string(r#""\x1a""#)?, r#""\u001a""#);
    assert_eq!(&encode_string(r#""\x1b""#)?, r#""\u001b""#);
    assert_eq!(&encode_string(r#""\x1c""#)?, r#""\u001c""#);
    assert_eq!(&encode_string(r#""\x1d""#)?, r#""\u001d""#);
    assert_eq!(&encode_string(r#""\x1e""#)?, r#""\u001e""#);
    assert_eq!(&encode_string(r#""\x1f""#)?, r#""\u001f""#);

    pretty_assertions::assert_eq!(&encode_string(r#"{"\t": "\n"}"#)?, r#"{"\t":"\n"}"#);
    assert_eq!(&encode_string(r#""\\""#)?, r#""\\""#);
    assert_eq!(&encode_string(r#""\"""#)?, r#""\"""#);

    Ok(())
}

#[test]
fn ordered_nested_object() -> Result<(), String> {
    roundtrip(
        r#"{"a":1,"b":2,"c":{"a":null,"h":{"h":-5,"i":3},"x":{}},"nested":{"bad":true,"good":false},"zzz":"I have a newline\n"}"#,
    )?;

    assert_eq!(
            r#"{
                "nested": {
                    "good": false,
                    "bad": true
                },
                "b": 2,
                "a": 1,
                "c": {
                    "h": {
                        "h": -5,
                        "i": 3
                    },
                    "a": null,
                    "x": {}
                },
                "zzz": "I have a newline\n"
            }"#.parse::<Value>()?.canonical_form().unwrap(),
            br#"{"a":1,"b":2,"c":{"a":null,"h":{"h":-5,"i":3},"x":{}},"nested":{"bad":true,"good":false},"zzz":"I have a newline\n"}"#.to_vec(),
        );

    Ok(())
}

#[test]
fn foo_canon() {
    let val = Foo {
        x_foo: 42,
        y_foo: Some("hello".into()),
    };
    assert_eq!(
        val.into_cjson(),
        vec![(
            "Foo".into(),
            vec![
                ("xFoo".into(), 42u64.into_cjson()),
                ("yFoo".into(), "hello".into_cjson())
            ]
            .into_iter()
            .collect::<Map>()
            .into_cjson()
        )]
        .into_iter()
        .collect::<Map>()
        .into_cjson()
    );
}

#[test]
fn bar_canon() {
    let val = Bar(true, false);
    assert_eq!(
        val.into_cjson(),
        vec![(
            "Bar".into(),
            vec![true, false]
                .into_iter()
                .collect::<Array>()
                .into_cjson()
        )]
        .into_iter()
        .collect::<Map>()
        .into_cjson()
    );
}

#[test]
fn baz_canon() {
    assert_eq!(Baz.into_cjson(), Value::Null);
}

#[test]
fn e_canon() {
    let val = E::W { a: 42, b: -3 };
    assert_eq!(
        val.into_cjson(),
        vec![(
            "W".into(),
            vec![
                ("a".into(), 42u64.into_cjson()),
                ("b".into(), (-3).into_cjson()),
            ]
            .into_iter()
            .collect::<Map>()
            .into_cjson()
        )]
        .into_iter()
        .collect::<Map>()
        .into_cjson()
    );

    let val = E::X(42, 3);
    assert_eq!(
        val.into_cjson(),
        vec![(
            "X".into(),
            vec![42u64.into_cjson(), 3.into_cjson()]
                .into_iter()
                .collect::<Array>()
                .into_cjson(),
        )]
        .into_iter()
        .collect::<Map>()
        .into_cjson()
    );

    let val = E::Y(42);
    assert_eq!(
        val.into_cjson(),
        vec![(
            "Y".into(),
            vec![42.into_cjson()]
                .into_iter()
                .collect::<Array>()
                .into_cjson()
        )]
        .into_iter()
        .collect::<Map>()
        .into_cjson()
    );

    assert_eq!(E::Z.into_cjson(), Value::String("Z".into()))
}
