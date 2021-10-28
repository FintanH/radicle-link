use std::{convert::TryFrom, str};

use git_repository::easy::{object::Kind, ObjectRef};

use link_canonical::{
    json::{Map, ToCjson, Value},
    Canonical,
    Cstring,
};

use crate::config::{self, Cobs, Data};

pub type Config = config::Config<Cstring, Cstring>;

pub mod error {
    use git_repository::ObjectId;
    use thiserror::Error;

    use crate::config::{cobs, data};

    #[derive(Debug, Error)]
    pub enum Json {
        #[error("missing `\"{0}\"` key")]
        MissingKey(String),
        #[error("expected type {expected}, but found {found}")]
        MismatchedTy { expected: String, found: String },
        #[error(transparent)]
        Cobs(#[from] cobs::cjson::error::Cobs),
        #[error(transparent)]
        Data(#[from] data::cjson::Error),
    }

    #[derive(Debug, Error)]
    pub enum Git {
        #[error("could not parse config at {oid}")]
        Parse {
            oid: ObjectId,
            #[source]
            reason: Box<dyn std::error::Error + 'static + Send + Sync>,
        },
        #[error(transparent)]
        Config(#[from] Json),
    }
}

impl<Id: ToCjson + Ord> ToCjson for config::Config<Cstring, Id> {
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

impl<Id: ToCjson + Ord + Clone> Canonical for config::Config<Cstring, Id> {
    type Error = <Value as Canonical>::Error;

    fn canonical_form(&self) -> Result<Vec<u8>, Self::Error> {
        self.clone().into_cjson().canonical_form()
    }
}

impl TryFrom<&Value> for config::Config<Cstring, Cstring> {
    type Error = error::Json;

    fn try_from(val: &Value) -> Result<Self, Self::Error> {
        use error::Json as Error;

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

impl<'a, A> TryFrom<&'a ObjectRef<'a, A>> for config::Config<Cstring, Cstring> {
    type Error = error::Git;

    fn try_from(obj: &'a ObjectRef<'a, A>) -> Result<Self, Self::Error> {
        use error::Git as Error;

        debug_assert!(obj.kind == Kind::Blob);

        let val = str::from_utf8(obj.as_ref())
            .unwrap()
            .parse::<Value>()
            .map_err(|reason| Error::Parse {
                oid: obj.id,
                reason: reason.into(),
            })?;
        Ok(Self::try_from(&val)?)
    }
}
