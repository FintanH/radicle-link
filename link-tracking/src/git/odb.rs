// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::TryInto;

use crate::git::config;

pub trait Read {
    type FindError: std::error::Error + Send + Sync + 'static;
    type ConfigError: std::error::Error + Send + Sync + 'static;

    type Oid;
    type Blob: TryInto<config::Config, Error = Self::ConfigError>;

    fn find_blob(&self, oid: &Self::Oid) -> Result<Option<Self::Blob>, Self::FindError>;
}

pub trait Write {
    type WriteError: std::error::Error + Send + Sync + 'static;

    type Oid;

    fn write_object(&self, bytes: Vec<u8>) -> Result<Self::Oid, Self::WriteError>;
}
