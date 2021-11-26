// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::TryFrom;

use link_canonical::json;
use link_tracking::git::{
    config,
    odb::{Read, Write},
};

use crate::{
    git::storage::{ReadOnly, ReadOnlyStorage as _, Storage},
    git_ext as ext,
};

pub struct Blob(Vec<u8>);

impl TryFrom<Blob> for config::Config {
    type Error = error::Config;

    fn try_from(Blob(bytes): Blob) -> Result<Self, Self::Error> {
        let value = json::Value::try_from(bytes.as_slice()).map_err(error::Config::Parse)?;
        Ok(link_tracking::config::Config::try_from(value)?)
    }
}

pub mod error {
    use thiserror::Error;

    use crate::{git::storage::read, git_ext as ext};

    #[derive(Debug, Error)]
    pub enum Find {
        #[error("the object at {0} is not a blob")]
        NotBlob(ext::Oid),
        #[error(transparent)]
        Read(#[from] read::Error),
    }

    #[derive(Debug, Error)]
    pub enum Write {
        #[error(transparent)]
        Git(#[from] git2::Error),
    }

    #[derive(Debug, Error)]
    pub enum Config {
        #[error("failed to parse Canonical JSON: {0}")]
        Parse(String),
        #[error(transparent)]
        Json(#[from] link_tracking::config::Error),
    }
}

impl Read for ReadOnly {
    type FindError = error::Find;
    type ConfigError = error::Config;

    type Oid = ext::Oid;
    type Blob = Blob;

    fn find_blob(&self, oid: &Self::Oid) -> Result<Option<Self::Blob>, Self::FindError> {
        let content = {
            match self.find_object(oid)? {
                None => None,
                Some(obj) => {
                    let blob = obj.as_blob().ok_or_else(|| error::Find::NotBlob(*oid))?;
                    Some(blob.content().to_vec())
                },
            }
        };

        Ok(content.map(Blob))
    }
}

impl Read for Storage {
    type FindError = error::Find;
    type ConfigError = error::Config;

    type Oid = ext::Oid;
    type Blob = Blob;

    fn find_blob(&self, oid: &Self::Oid) -> Result<Option<Self::Blob>, Self::FindError> {
        self.read_only().find_blob(oid)
    }
}

impl Write for Storage {
    type WriteError = error::Write;

    type Oid = ext::Oid;

    fn write_object(&self, bytes: Vec<u8>) -> Result<Self::Oid, Self::WriteError> {
        Ok(self.as_raw().blob(&bytes).map(ext::Oid::from)?)
    }
}
