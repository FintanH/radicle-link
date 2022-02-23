// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use librad::PeerId;

use super::Seed;

pub mod file;
pub use file::{Iter, KVStore};

/// Key-value storage for [`Seed`]s, where the key is given by the
/// [`PeerId`] and the value is the network address.
pub trait Store<Addrs> {
    type Get: std::error::Error + Send + Sync + 'static;
    type Exists: std::error::Error + Send + Sync + 'static;
    type Insert: std::error::Error + Send + Sync + 'static;
    type Remove: std::error::Error + Send + Sync + 'static;

    /// Retrieve the [`Seed`] by its [`PeerId`].
    fn get(&self, peer: PeerId) -> Result<Option<Seed<Addrs>>, Self::Get>;

    /// Check that a [`Seed`] exists for the given [`PeerId`].
    fn exists(&self, peer: PeerId) -> Result<bool, Self::Exists>;

    /// Insert a [`Seed`] into the storage.
    ///
    /// If a seed already existed for the [`PeerId`], then the old value is
    /// returned. Otherwise, `None` is returned if it is a new entry.
    fn insert(&mut self, seed: Seed<Addrs>) -> Result<Option<Seed<Addrs>>, Self::Insert>;

    /// Remove the [`Seed`] given by [`PeerId`].
    ///
    /// Returns `true` if the `peer` was present and it was removed.
    /// Returns `false` if the `peer` did not exist.    
    fn remove(&mut self, peer: PeerId) -> Result<bool, Self::Remove>;
}

/// Get an iterator of the [`Seed`] in the [`Store`].
pub trait Scan<Addrs> {
    type Scan: std::error::Error + Send + Sync + 'static;
    type Iter: std::error::Error + Send + Sync + 'static;

    type Seeds: Iterator<Item = Result<Seed<Addrs>, Self::Iter>>;

    /// Retrieve all [`Seed`]s in the storage.
    fn scan(self) -> Result<Self::Seeds, Self::Scan>;
}
