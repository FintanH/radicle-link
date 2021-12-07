// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use radicle_git_ext::RefspecPattern;

use crate::git::tracking::reference::RefName;

/// A reference loaded from a reference database.
///
/// The reference is expected to be a direct reference that points to a blob
/// containing a [`crate::git::config::Config`].
#[derive(Debug)]
pub struct Ref<'a, Oid: ToOwned + Clone> {
    pub name: RefName<'a, Oid>,
    pub target: Oid,
}

pub trait Read {
    type FindError: std::error::Error + Send + Sync + 'static;
    type ReferencesError: std::error::Error + Send + Sync + 'static;
    type IterError: std::error::Error + Send + Sync + 'static;

    type Oid: Clone;

    /// Get a [`Ref`] by `name`, returning `None` if no such reference exists.
    fn find_reference(
        &self,
        name: &RefName<'_, Self::Oid>,
    ) -> Result<Option<Ref<Self::Oid>>, Self::FindError>;

    /// Get all [`Ref`]s that match the given `refspec`.
    #[allow(clippy::type_complexity)]
    fn references(
        &self,
        refspec: &RefspecPattern,
    ) -> Result<Vec<Result<Ref<Self::Oid>, Self::IterError>>, Self::ReferencesError>;
}

pub trait Write {
    type TxnError: std::error::Error + Send + Sync + 'static;

    type Oid: ToOwned + Clone;

    /// Apply the provided ref updates.
    ///
    /// This should be a transaction: either all updates are applied, or none.
    fn update<'a, I>(&self, updates: I) -> Result<Applied<'a, Self::Oid>, Self::TxnError>
    where
        I: IntoIterator<Item = Update<'a, Self::Oid>>;
}

#[derive(Clone, Debug)]
pub enum Update<'a, Oid: ToOwned + Clone> {
    Write { name: RefName<'a, Oid>, target: Oid },
    Delete { name: RefName<'a, Oid> },
}

pub struct Applied<'a, Oid: ToOwned + Clone> {
    pub updates: Vec<Updated<'a, Oid>>,
}

impl<'a, Oid: ToOwned + Clone> Default for Applied<'a, Oid> {
    fn default() -> Self {
        Applied {
            updates: Vec::new(),
        }
    }
}

impl<'a, Oid: ToOwned + Clone> Applied<'a, Oid> {
    pub fn append(&mut self, mut other: Vec<Updated<'a, Oid>>) {
        self.updates.append(&mut other)
    }
}

#[derive(Clone, Debug)]
pub enum Updated<'a, Oid: ToOwned + Clone> {
    Written { name: RefName<'a, Oid>, target: Oid },
    Deleted { name: RefName<'a, Oid> },
}
