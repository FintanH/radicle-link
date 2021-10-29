// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::{TryFrom, TryInto as _};

use link_canonical::{json::Value, Canonical, Cstring};
use link_tracking::config;

pub use config::cjson::Error;

pub type Cobs = config::Cobs<Cstring, Cstring>;

pub struct Config(config::Config<Cstring, Cstring>);

impl Default for Config {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl Canonical for Config {
    type Error = <config::Config<Cstring, Cstring> as Canonical>::Error;

    fn canonical_form(&self) -> Result<Vec<u8>, Self::Error> {
        self.0.canonical_form()
    }
}

impl TryFrom<&Value> for Config {
    type Error = config::cjson::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}
