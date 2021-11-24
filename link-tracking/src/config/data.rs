// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

/// Flag indicating whether non-rad data should be replicated or not.
#[derive(ToCjson, Clone, Debug)]
pub struct Data(pub bool);

impl Default for Data {
    fn default() -> Self {
        Self(true)
    }
}

pub mod cjson {
    use std::convert::TryFrom;

    use thiserror::Error;

    use link_canonical::json::Value;

    use super::Data;

    #[derive(Debug, Error)]
    pub enum Error {
        #[error("expected type {expected}, but found {found}")]
        MismatchedTy { expected: String, found: String },
    }

    impl TryFrom<&Value> for Data {
        type Error = Error;

        fn try_from(val: &Value) -> Result<Self, Self::Error> {
            match val {
                Value::Bool(flag) => Ok(Self(*flag)),
                val => Err(Error::MismatchedTy {
                    expected: "bool".to_string(),
                    found: val.ty_name().to_string(),
                }),
            }
        }
    }
}
