use std::{collections::BTreeMap, fmt};

#[cfg(feature = "cjson")]
use link_canonical::{
    json::{ToCjson, Value},
    Cstring,
};

pub trait Configure: Default {
    type Typename;
    type ObjectId;

    fn set_data(&mut self, data: Data);
    fn set_cobs(&mut self, cobs: Cobs<Self::Typename, Self::ObjectId>);
    fn filter_cob(&mut self, typename: Self::Typename, object: Object<Self::ObjectId>);
}

pub enum Object<Id> {
    Wildcard,
    Identifier(Id),
}

pub enum Key {
    Cobs,
    Data,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cobs => write!(f, "cobs"),
            Self::Data => write!(f, "data"),
        }
    }
}

#[cfg_attr(feature = "cjson", derive(ToCjson))]
pub struct Data(pub bool);

impl Default for Data {
    fn default() -> Self {
        Self(true)
    }
}

pub enum Cobs<Type, ObjectId> {
    WildCard,
    Filters(BTreeMap<Type, ObjectId>),
}

impl<T, O> Default for Cobs<T, O> {
    fn default() -> Self {
        Self::WildCard
    }
}

#[cfg(feature = "cjson")]
impl<ObjectId: ToCjson> ToCjson for Cobs<Cstring, ObjectId> {
    fn into_cjson(self) -> Value {
        match self {
            Self::WildCard => Value::String("*".into()),
            Self::Filters(filters) => filters.into_cjson(),
        }
    }
}
