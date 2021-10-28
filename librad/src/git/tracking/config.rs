// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use link_canonical::{json::Value, Canonical, Cstring};
use link_tracking::config::{self, Configure};

pub struct Config(pub(super) Value);

impl Canonical for Config {
    type Error = <Value as Canonical>::Error;

    fn canonical_form(&self) -> Result<Vec<u8>, Self::Error> {
        self.0.canonical_form()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self(todo!())
    }
}

impl Configure for Config {
    type Typename = Cstring;
    type ObjectId = Cstring;

    fn set_data(&mut self, data: config::Data) {
        todo!()
    }

    fn filter_cob(&mut self, typename: Self::Typename, object: config::Object<Self::ObjectId>) {
        todo!()
    }

    fn set_cobs(&mut self, cobs: config::Cobs<Self::Typename, Self::ObjectId>) {
        todo!()
    }
}
