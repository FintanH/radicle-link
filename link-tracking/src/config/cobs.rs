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
    Filters(BTreeMap<Type, BTreeSet<Filter<ObjectId>>>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ToCjson)]
pub struct Filter<ObjectId> {
    pub policy: Policy,
    pub pattern: Object<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Policy {
    Allow,
    Deny,
}

impl<Ty: Ord, Id: Ord> Cobs<Ty, Id> {
    /// Insert an `Object` for the given `typename`.
    ///
    /// # Note
    ///
    /// If `self` is [`Cobs::Wildcard`], it will be turned into a
    /// [`Cobs::Filters`] using the the `typename` and `Object` as the
    /// initial entries.
    pub fn insert(&mut self, typename: Ty, filter: Filter<Id>)
    where
        Id: Clone,
    {
        match self {
            Self::Wildcard => {
                let mut filters = BTreeMap::new();
                filters.insert(typename, vec![filter].into_iter().collect());
                *self = Self::Filters(filters);
            },
            Self::Filters(filters) => {
                filters
                    .entry(typename)
                    .and_modify(|objs| {
                        objs.insert(filter.clone());
                    })
                    .or_insert_with(|| vec![filter].into_iter().collect());
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
    pub fn remove(&mut self, typename: &Ty, filter: &Filter<Id>) {
        match self {
            Self::Wildcard => { /* no-op */ },
            Self::Filters(filters) => {
                if let Some(objs) = filters.get_mut(typename) {
                    objs.remove(filter);
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

impl<Ty: Ord, Id: Ord> FromIterator<(Ty, BTreeSet<Filter<Id>>)> for Cobs<Ty, Id> {
    fn from_iter<T: IntoIterator<Item = (Ty, BTreeSet<Filter<Id>>)>>(iter: T) -> Self {
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

// TODO(finto): put into its own file
pub mod cjson {
    use std::{
        collections::BTreeSet,
        convert::{TryFrom, TryInto},
    };

    use link_canonical::{
        json::{ToCjson, Value},
        Cstring,
    };

    use super::{Cobs, Filter, Object, Policy};

    const POLICY: &str = "policy";
    const PATTERN: &str = "pattern";

    pub mod error {
        use thiserror::Error;

        use link_canonical::Cstring;

        #[derive(Debug, Error)]
        pub enum Policy {
            #[error(r#"expected `"allow"` or `"deny"`, but found {0}"#)]
            Unexpected(Cstring),
            #[error("expected type {expected}, but found {found}")]
            MismatchedTy { expected: String, found: String },
        }

        #[derive(Debug, Error)]
        pub enum Object {
            #[error("expected type {expected}, but found {found}")]
            MismatchedTy { expected: String, found: String },
            #[error("failed to parse the object identifier")]
            Identifier(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
        }

        #[derive(Debug, Error)]
        pub enum Filter {
            #[error(r#"missing key `"{0}"`"#)]
            Missing(&'static str),
            #[error("expected type {expected}, but found {found}")]
            MismatchedTy { expected: String, found: String },
            #[error(transparent)]
            Policy(#[from] Policy),
            #[error(transparent)]
            Object(#[from] Object),
        }

        #[derive(Debug, Error)]
        pub enum Cobs {
            #[error("expected type {expected}, but found {found}")]
            MismatchedTy { expected: String, found: String },
            #[error("expected `\"*\"`, but found {0}")]
            MismatchedStr(String),
            #[error(transparent)]
            Filter(#[from] Filter),
            #[error("failed to parse the object's type name")]
            TypeName(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
        }
    }

    impl ToCjson for Policy {
        fn into_cjson(self) -> Value {
            match self {
                Self::Allow => "allow".into_cjson(),
                Self::Deny => "deny".into_cjson(),
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

    impl<Ty: Into<Cstring> + Ord, ObjectId: ToCjson + Ord> ToCjson for Cobs<Ty, ObjectId> {
        fn into_cjson(self) -> Value {
            match self {
                Self::Wildcard => Value::String("*".into()),
                Self::Filters(filters) => filters.into_cjson(),
            }
        }
    }

    impl<Id> TryFrom<Value> for Filter<Id>
    where
        Value: TryInto<Id>,
        <Value as TryInto<Id>>::Error: std::error::Error + Send + Sync + 'static,
    {
        type Error = error::Filter;

        fn try_from(value: Value) -> Result<Self, Self::Error> {
            match value {
                Value::Object(map) => {
                    let policy = map
                        .get(&POLICY.into())
                        .ok_or(error::Filter::Missing(POLICY))?;
                    let pattern = map
                        .get(&PATTERN.into())
                        .ok_or(error::Filter::Missing(PATTERN))?;

                    Ok(Self {
                        policy: Policy::try_from(policy)?,
                        pattern: Object::try_from(pattern.clone())?,
                    })
                },
                val => Err(error::Filter::MismatchedTy {
                    expected: r#"expected string `"allow"` or `"deny"`"#.to_string(),
                    found: val.ty_name().to_string(),
                }),
            }
        }
    }

    impl TryFrom<&Value> for Policy {
        type Error = error::Policy;

        fn try_from(value: &Value) -> Result<Self, Self::Error> {
            match value {
                Value::String(policy) => match policy.as_str() {
                    "allow" => Ok(Self::Allow),
                    "deny" => Ok(Self::Deny),
                    _ => Err(error::Policy::Unexpected(policy.clone())),
                },
                val => Err(error::Policy::MismatchedTy {
                    expected: r#"expected string `"allow"` or `"deny"`"#.to_string(),
                    found: val.ty_name().to_string(),
                }),
            }
        }
    }

    impl<Id> TryFrom<Value> for Object<Id>
    where
        Value: TryInto<Id>,
        <Value as TryInto<Id>>::Error: std::error::Error + Send + Sync + 'static,
    {
        type Error = error::Object;

        fn try_from(value: Value) -> Result<Self, Self::Error> {
            match &value {
                Value::String(s) => match s.as_str() {
                    "*" => Ok(Self::Wildcard),
                    _ => value
                        .try_into()
                        .map(Self::Identifier)
                        .map_err(|err| error::Object::Identifier(err.into())),
                },
                val => Err(error::Object::MismatchedTy {
                    expected: "string of `\"*\"` or `<object id>`".into(),
                    found: val.ty_name().to_string(),
                }),
            }
        }
    }

    impl<Ty, Id> TryFrom<&Value> for Cobs<Ty, Id>
    where
        Ty: Ord,
        Id: Ord,
        Value: TryInto<Id>,
        Cstring: TryInto<Ty>,
        <Cstring as TryInto<Ty>>::Error: std::error::Error + Send + Sync + 'static,
        <Value as TryInto<Id>>::Error: std::error::Error + Send + Sync + 'static,
    {
        type Error = error::Cobs;

        fn try_from(value: &Value) -> Result<Self, Self::Error> {
            match value {
                Value::Object(cobs) => cobs
                    .iter()
                    .map(|(typename, objects)| match objects {
                        Value::Array(objs) => {
                            let typename = typename
                                .clone()
                                .try_into()
                                .map_err(|err| error::Cobs::TypeName(err.into()));
                            typename.and_then(|ty| {
                                objs.iter()
                                    .cloned()
                                    .map(Filter::try_from)
                                    .collect::<Result<BTreeSet<_>, _>>()
                                    .map(|objs| (ty, objs))
                                    .map_err(error::Cobs::from)
                            })
                        },
                        val => Err(error::Cobs::MismatchedTy {
                            expected: "[<object id>...]".to_string(),
                            found: val.ty_name().to_string(),
                        }),
                    })
                    .collect::<Result<Cobs<Ty, Id>, _>>(),
                Value::String(s) => match s.as_str() {
                    "*" => Ok(Self::Wildcard),
                    _ => Err(error::Cobs::MismatchedStr(s.to_string())),
                },
                val => Err(error::Cobs::MismatchedTy {
                    expected: r#"{"<typename>": [<object id>...]}"#.to_string(),
                    found: val.ty_name().to_string(),
                }),
            }
        }
    }
}
