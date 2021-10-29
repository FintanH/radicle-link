use std::{collections::BTreeMap, iter::FromIterator};

pub trait Configure: Default {
    type Typename;
    type ObjectId;

    fn set(&mut self, cobs: Cobs<Self::Typename, Self::ObjectId>);
    fn insert(&mut self, typename: Self::Typename, object: Object<Self::ObjectId>);
    fn remove(&mut self, typename: Self::Typename, object: Object<Self::ObjectId>);
    fn remove_type(&mut self, typename: &Self::Typename);
}

#[derive(Clone, Debug)]
pub enum Cobs<Type, ObjectId> {
    WildCard,
    Filters(BTreeMap<Type, Vec<Object<ObjectId>>>),
}

impl<T, O> Default for Cobs<T, O> {
    fn default() -> Self {
        Self::WildCard
    }
}

impl<Ty: Ord, Id> FromIterator<(Ty, Vec<Object<Id>>)> for Cobs<Ty, Id> {
    fn from_iter<T: IntoIterator<Item = (Ty, Vec<Object<Id>>)>>(iter: T) -> Self {
        Self::Filters(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone)]
pub enum Object<Id> {
    Wildcard,
    Identifier(Id),
}

#[cfg(feature = "cjson")]
pub mod cjson {
    use std::convert::TryFrom;

    use link_canonical::{
        json::{ToCjson, Value},
        Cstring,
    };

    use super::{Cobs, Object};

    pub mod error {
        use thiserror::Error;

        #[derive(Debug, Error)]
        pub enum Object {
            #[error("expected type {expected}, but found {found}")]
            MismatchedTy { expected: String, found: String },
        }

        #[derive(Debug, Error)]
        pub enum Cobs {
            #[error("expected type {expected}, but found {found}")]
            MismatchedTy { expected: String, found: String },
            #[error(transparent)]
            Object(#[from] Object),
        }
    }

    impl<ObjectId: ToCjson> ToCjson for Cobs<Cstring, ObjectId> {
        fn into_cjson(self) -> Value {
            match self {
                Self::WildCard => Value::String("*".into()),
                Self::Filters(filters) => filters.into_cjson(),
            }
        }
    }

    impl<Id: ToCjson> ToCjson for Object<Id> {
        fn into_cjson(self) -> Value {
            match self {
                Self::Wildcard => "*".into_cjson(),
                Self::Identifier(id) => id.into_cjson(),
            }
        }
    }

    impl TryFrom<Value> for Object<Cstring> {
        type Error = error::Object;

        fn try_from(value: Value) -> Result<Self, Self::Error> {
            match value {
                Value::String(s) => match s.as_str() {
                    "*" => Ok(Self::Wildcard),
                    _ => Ok(Self::Identifier(s)),
                },
                val => Err(error::Object::MismatchedTy {
                    expected: "string of `\"*\"` or `<object id>`".into(),
                    found: val.ty_name(),
                }),
            }
        }
    }

    impl TryFrom<&Value> for Cobs<Cstring, Cstring> {
        type Error = error::Cobs;

        fn try_from(value: &Value) -> Result<Self, Self::Error> {
            match value {
                Value::Object(cobs) => cobs
                    .iter()
                    .map(|(typename, objects)| match objects {
                        Value::Array(objs) => objs
                            .iter()
                            .cloned()
                            .map(Object::try_from)
                            .collect::<Result<Vec<_>, _>>()
                            .map(|objs| (typename.clone(), objs))
                            .map_err(error::Cobs::from),
                        val => Err(error::Cobs::MismatchedTy {
                            expected: "[<object id>...]".to_string(),
                            found: val.ty_name(),
                        }),
                    })
                    .collect::<Result<Cobs<Cstring, Cstring>, _>>(),
                val => {
                    return Err(error::Cobs::MismatchedTy {
                        expected: r#"{"<typename>": [<object id>...]}"#.to_string(),
                        found: val.ty_name(),
                    })
                },
            }
        }
    }
}
