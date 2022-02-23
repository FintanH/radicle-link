// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{
    convert::Infallible,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use pickledb::{PickleDb, PickleDbDumpPolicy, PickleDbIterator};

use librad::{crypto::peer, paths::Paths, PeerId};
use serde::{de::DeserializeOwned, Serialize};

use super::{Scan, Seed, Store};

pub const FILE_NAME: &str = "lnk-seeds";

pub fn default_path(paths: &Paths) -> PathBuf {
    paths.seeds_dir().join(FILE_NAME)
}

/// A key-value store backed by a [`PickleDb`].
///
/// The storage format is JSON.
pub struct KVStore {
    pickle: PickleDb,
}

impl KVStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, error::Load> {
        let policy = PickleDbDumpPolicy::AutoDump;
        if path.as_ref().exists() {
            Ok(Self {
                pickle: PickleDb::load_json(path, policy)?,
            })
        } else {
            Ok(Self {
                pickle: PickleDb::new_json(path, policy),
            })
        }
    }

    pub fn iter<T>(&self) -> Iter<'_, T> {
        Iter {
            inner: self.pickle.iter(),
            _marker: PhantomData,
        }
    }
}

pub struct Iter<'a, T> {
    inner: PickleDbIterator<'a>,
    _marker: PhantomData<T>,
}

impl<'a, T> Iterator for Iter<'a, T>
where
    T: DeserializeOwned,
{
    type Item = Result<Seed<T>, peer::conversion::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().and_then(|kv| {
            let addrs = kv.get_value::<T>();
            addrs.map(|addrs| kv.get_key().parse().map(|peer| Seed { peer, addrs }))
        })
    }
}

impl<T: Serialize + DeserializeOwned> Store<T> for KVStore {
    type Get = Infallible;
    type Exists = Infallible;
    type Remove = error::Remove;
    type Insert = error::Insert;

    fn get(&self, peer: PeerId) -> Result<Option<super::Seed<T>>, Self::Get> {
        Ok(self
            .pickle
            .get(&peer.to_string())
            .map(|addrs| Seed { peer, addrs }))
    }

    fn exists(&self, peer: PeerId) -> Result<bool, Self::Exists> {
        Ok(self.pickle.exists(&peer.to_string()))
    }

    fn insert(&mut self, seed: super::Seed<T>) -> Result<Option<Seed<T>>, Self::Insert> {
        let existing = self.get(seed.peer).unwrap();
        self.pickle
            .set(&seed.peer.to_string(), &seed.addrs)
            .map_err(|err| error::Insert {
                peer: seed.peer,
                source: err,
            })?;
        Ok(existing)
    }

    fn remove(&mut self, peer: PeerId) -> Result<bool, Self::Remove> {
        self.pickle
            .rem(&peer.to_string())
            .map_err(|err| error::Remove { peer, source: err })
    }
}

impl<'a, T> Scan<T> for &'a KVStore
where
    T: DeserializeOwned,
{
    type Scan = Infallible;
    type Iter = peer::conversion::Error;
    type Seeds = Iter<'a, T>;

    fn scan(self) -> Result<Self::Seeds, Self::Scan> {
        Ok(self.iter())
    }
}

pub mod error {
    use librad::PeerId;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("failed to load storage")]
    pub struct Load(#[from] pickledb::error::Error);

    #[derive(Debug, Error)]
    #[error("failed to insert seed for peer `{peer}`")]
    pub struct Insert {
        pub peer: PeerId,
        #[source]
        pub source: pickledb::error::Error,
    }

    #[derive(Debug, Error)]
    #[error("failed to remove seed for peer `{peer}`")]
    pub struct Remove {
        pub peer: PeerId,
        #[source]
        pub source: pickledb::error::Error,
    }
}
