#[cfg_attr(feature = "cjson", derive(ToCjson))]
#[derive(Clone, Debug)]
pub struct Data(pub bool);

impl Default for Data {
    fn default() -> Self {
        Self(true)
    }
}

#[cfg(feature = "cjson")]
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
                    found: val.ty_name(),
                }),
            }
        }
    }
}
