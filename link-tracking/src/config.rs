// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

pub mod cobs;
pub mod data;

pub use cobs::{Cobs, Object};
pub use data::Data;

#[derive(Clone, Debug)]
pub struct Config<Typename, ObjectId> {
    pub data: Data,
    pub cobs: Cobs<Typename, ObjectId>,
}

impl<Ty, Id> Default for Config<Ty, Id> {
    fn default() -> Self {
        Self {
            data: Data::default(),
            cobs: Cobs::default(),
        }
    }
}

/*
pub mod cjson {
    use std::convert::TryFrom;

    use link_canonical::{
        json::{Map, ToCjson, Value},
        Canonical,
        Cstring,
    };

    use super::{cobs, data, Cobs, Config, Data};

    #[derive(Debug, Error)]
    pub enum Error {
        #[error("missing `\"{0}\"` key")]
        MissingKey(String),
        #[error("expected type {expected}, but found {found}")]
        MismatchedTy { expected: String, found: String },
        #[error(transparent)]
        Cobs(#[from] cobs::cjson::error::Cobs),
        #[error(transparent)]
        Data(#[from] data::cjson::Error),
    }

    impl<Id: ToCjson + Ord> ToCjson for Config<Cstring, Id> {
        fn into_cjson(self) -> Value {
            Value::Object(
                vec![
                    ("data".into(), self.data.into_cjson()),
                    ("cobs".into(), self.cobs.into_cjson()),
                ]
                .into_iter()
                .collect::<Map>(),
            )
        }
    }

    impl<Id: ToCjson + Ord + Clone> Canonical for Config<Cstring, Id> {
        type Error = <Value as Canonical>::Error;

        fn canonical_form(&self) -> Result<Vec<u8>, Self::Error> {
            self.clone().into_cjson().canonical_form()
        }
    }

    impl TryFrom<&Value> for Config<Cstring, Cstring> {
        type Error = Error;

        fn try_from(val: &Value) -> Result<Self, Self::Error> {
            const COBS_KEY: &str = "cobs";
            const DATA_KEY: &str = "data";

            match val {
                Value::Object(map) => {
                    let cobs = map
                        .get(&COBS_KEY.into())
                        .ok_or_else(|| Error::MissingKey(COBS_KEY.into()))?;
                    let data = map
                        .get(&DATA_KEY.into())
                        .ok_or_else(|| Error::MissingKey(DATA_KEY.into()))?;

                    let data = Data::try_from(data)?;
                    let cobs = Cobs::try_from(cobs)?;
                    Ok(Self { data, cobs })
                },
                val => Err(Error::MismatchedTy {
                    expected: "object, keys: [\"cobs\", \"data\"]".to_string(),
                    found: val.ty_name(),
                }),
            }
        }
    }
}
*/
