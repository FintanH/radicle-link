// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use crate::git::config::Config;

pub trait Read {
    type FindError: std::error::Error + Send + Sync + 'static;
    type ConfigError: std::error::Error + Send + Sync + 'static;

    type Oid;

    fn find_blob(&self, oid: &Self::Oid) -> Result<Option<Config>, Self::FindError>;
}

pub trait Write {
    type WriteError: std::error::Error + Send + Sync + 'static;

    type Oid;

    fn write_object(&self, config: &Config) -> Result<Self::Oid, Self::WriteError>;
}
