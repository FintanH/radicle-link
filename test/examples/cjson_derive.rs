// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use link_canonical::{json::Cjson, Cstring};
use link_canonical_derive::Cjson;

#[derive(Cjson)]
struct Foo {
    x: u64,
    y: Option<Cstring>,
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
        x: 42,
        y: Some("hello".into()),
    };
    println!("{:#?}", val.into_cjson());

    let val = Bar(true, false);
    println!("{:#?}", val.into_cjson());

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
