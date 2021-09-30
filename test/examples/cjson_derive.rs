// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::str;

use link_canonical::{json::Cjson, Canonical as _, Cstring};
use link_canonical_derive::Cjson;

#[derive(Cjson)]
#[cjson(rename_all = "camelCase")]
struct Foo {
    x_foo: u64,
    y_foo: Option<Cstring>,
}

#[derive(Cjson)]
struct Bar(bool, bool);

#[derive(Cjson)]
struct Baz;

#[derive(Cjson)]
enum E {
    W { a: i32, b: i32 },
    X(i32, i32),
    Y(i32),
    Z,
}

#[derive(Cjson)]
enum Union {
    Ize,
    Jack,
}

fn main() -> anyhow::Result<()> {
    let val = Foo {
        x_foo: 42,
        y_foo: Some("hello".into()),
    };
    let val = val.into_cjson();
    println!("{:#?}", val);
    println!(
        "{}",
        str::from_utf8(&val.canonical_form().unwrap()).unwrap()
    );

    let val = Bar(true, false);
    let val = val.into_cjson();
    println!("{:#?}", val);
    println!(
        "{}",
        str::from_utf8(&val.canonical_form().unwrap()).unwrap()
    );

    let val = Baz;
    println!("{:#?}", val.into_cjson());

    let val = E::W { a: 0, b: 0 };
    println!("{:#?}", val.into_cjson());

    let val = E::X(0, 0);
    println!("{:#?}", val.into_cjson());

    let val = E::Y(0);
    println!("{:#?}", val.into_cjson());

    let val = E::Z;
    println!("{:#?}", val.into_cjson());

    let val = Union::Ize;
    println!("{:#?}", val.into_cjson());

    let val = Union::Jack;
    println!("{:#?}", val.into_cjson());

    Ok(())
}
