// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    collections::{BTreeMap, BTreeSet},
    iter::FromIterator,
};

/// Either a wildcard `*` or a set of filters.
///
/// The filters are keyed by the Collaborative Object typename where the value
/// is a set of Collaborative Object Identifiers or a wildcard `*`.
#[derive(Clone, Debug)]
pub enum Cobs<Type, ObjectId> {
    Wildcard,
    Filters(BTreeMap<Type, BTreeSet<Object<ObjectId>>>),
}

impl<Ty: Ord, Id: Ord> Cobs<Ty, Id> {
    /// Insert an `Object` for the given `typename`.
    ///
    /// # Note
    ///
    /// If `self` is [`Cobs::Wildcard`], it will be turned into a
    /// [`Cobs::Filters`] using the the `typename` and `Object` as the
    /// initial entries.
    pub fn insert(&mut self, typename: Ty, obj: Object<Id>)
    where
        Id: Clone,
    {
        match self {
            Self::Wildcard => {
                let mut filters = BTreeMap::new();
                filters.insert(typename, vec![obj].into_iter().collect());
                *self = Self::Filters(filters);
            },
            Self::Filters(filters) => {
                filters
                    .entry(typename)
                    .and_modify(|objs| {
                        objs.insert(obj.clone());
                    })
                    .or_insert_with(|| vec![obj].into_iter().collect());
            },
        }
    }

    /// Remove the `Object` for the given `typename`.
    ///
    /// # Note
    ///
    /// If `self` is [`Cobs::Wildcard`] then this is a no-op.
    /// If the resulting set of objects is empty we remove the `typename` from
    /// the filters.
    pub fn remove(&mut self, typename: &Ty, obj: &Object<Id>) {
        match self {
            Self::Wildcard => { /* no-op */ },
            Self::Filters(filters) => {
                if let Some(objs) = filters.get_mut(typename) {
                    objs.remove(obj);
                    if objs.is_empty() {
                        filters.remove(typename);
                    }
                }
            },
        }
    }

    /// Remove the given `typename` from the filters.
    pub fn remove_type(&mut self, typename: &Ty) {
        match self {
            Self::Wildcard => { /* no-op */ },
            Self::Filters(filters) => {
                filters.remove(typename);
            },
        }
    }
}

impl<T, O> Default for Cobs<T, O> {
    fn default() -> Self {
        Self::Wildcard
    }
}

impl<Ty: Ord, Id: Ord> FromIterator<(Ty, BTreeSet<Object<Id>>)> for Cobs<Ty, Id> {
    fn from_iter<T: IntoIterator<Item = (Ty, BTreeSet<Object<Id>>)>>(iter: T) -> Self {
        Self::Filters(iter.into_iter().collect())
    }
}

/// Either a wildcard `*` or a Collaborative Object Identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Object<Id> {
    Wildcard,
    Identifier(Id),
}

impl<Id> Object<Id> {
    pub fn map<O>(self, f: impl FnOnce(Id) -> O) -> Object<O> {
        match self {
            Self::Wildcard => Object::Wildcard,
            Self::Identifier(id) => Object::Identifier(f(id)),
        }
    }
}

pub mod cjson {
    use std::{collections::BTreeSet, convert::TryFrom};

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

    impl<Id: ToCjson> ToCjson for Object<Id> {
        fn into_cjson(self) -> Value {
            match self {
                Self::Wildcard => "*".into_cjson(),
                Self::Identifier(id) => id.into_cjson(),
            }
        }
    }

    impl<ObjectId: ToCjson + Ord> ToCjson for Cobs<Cstring, ObjectId> {
        fn into_cjson(self) -> Value {
            match self {
                Self::Wildcard => Value::String("*".into()),
                Self::Filters(filters) => filters.into_cjson(),
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
                            .collect::<Result<BTreeSet<_>, _>>()
                            .map(|objs| (typename.clone(), objs))
                            .map_err(error::Cobs::from),
                        val => Err(error::Cobs::MismatchedTy {
                            expected: "[<object id>...]".to_string(),
                            found: val.ty_name(),
                        }),
                    })
                    .collect::<Result<Cobs<Cstring, Cstring>, _>>(),
                val => Err(error::Cobs::MismatchedTy {
                    expected: r#"{"<typename>": [<object id>...]}"#.to_string(),
                    found: val.ty_name(),
                }),
            }
        }
    }
}
