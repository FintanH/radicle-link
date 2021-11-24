// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::TryFrom;

use thiserror::Error;

use link_canonical::{
    json::{ToCjson, Value},
    Canonical,
    Cstring,
};

pub mod cobs;
pub mod data;

pub use cobs::{Cobs, Object};
pub use data::Data;

const COBS: &str = "cobs";
const DATA: &str = "data";

/// Configuration to act as a set of filters for non-`rad` references.
#[derive(Clone, Debug)]
pub struct Config<Typename, ObjectId> {
    /// The regular git-set of references, ie. `heads`, `tags`, and `notes` are
    /// considered data-refs. `data` dictates the the filtering of these
    /// data-refs.
    pub data: Data,
    /// Filter collaborative objects based on their type name, object
    /// identifier, and a filtering policy.
    pub cobs: Cobs<Typename, ObjectId>,
}

impl<Ty: Into<Cstring> + Ord, Id: ToCjson + Ord> ToCjson for Config<Ty, Id> {
    fn into_cjson(self) -> Value {
        vec![
            ("data".into(), self.data.into_cjson()),
            ("cobs".into(), self.cobs.into_cjson()),
        ]
        .into_iter()
        .collect()
    }
}

impl<Ty: Clone + Ord + Into<Cstring> + Ord, Id: Clone + Ord + ToCjson> Canonical
    for Config<Ty, Id>
{
    type Error = <Value as Canonical>::Error;

    fn canonical_form(&self) -> Result<Vec<u8>, Self::Error> {
        self.clone().into_cjson().canonical_form()
    }
}

impl<Ty, Id> Default for Config<Ty, Id> {
    fn default() -> Self {
        Self {
            data: Data::default(),
            cobs: Cobs::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("expected type {expected}, but found {found}")]
    MismatchedTy { expected: String, found: String },
    #[error("missing `\"{0}\"` key")]
    Missing(&'static str),
    #[error(transparent)]
    Data(#[from] data::cjson::Error),
    #[error(transparent)]
    Cobs(#[from] cobs::cjson::error::Cobs),
}

impl<Ty, Id> TryFrom<Value> for Config<Ty, Id>
where
    Ty: TryFrom<Cstring> + Ord,
    Id: TryFrom<Value> + Ord,
    <Ty as TryFrom<Cstring>>::Error: std::error::Error + Send + Sync + 'static,
    <Id as TryFrom<Value>>::Error: std::error::Error + Send + Sync + 'static,
{
    type Error = Error;

    fn try_from(val: Value) -> Result<Self, Self::Error> {
        match val {
            Value::Object(map) => {
                let cobs = map.get(&COBS.into()).ok_or(Error::Missing(COBS))?;
                let data = map.get(&DATA.into()).ok_or(Error::Missing(DATA))?;

                let data = Data::try_from(data)?;
                let cobs = Cobs::try_from(cobs)?;
                Ok(Self { data, cobs })
            },
            val => Err(Error::MismatchedTy {
                expected: "object, keys: [\"cobs\", \"data\"]".to_string(),
                found: val.ty_name().to_string(),
            }),
        }
    }
}
