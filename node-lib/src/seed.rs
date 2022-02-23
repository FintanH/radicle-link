// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{convert::TryFrom, fmt, io, net::SocketAddr, str::FromStr};

use librad::{net::discovery, PeerId};
use serde::{Deserialize, Serialize};
use tokio::net::{lookup_host, ToSocketAddrs};

pub mod store;
pub use store::{Scan, Store};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct Seed<Addrs> {
    /// The identifier for the `Seed`.
    pub peer: PeerId,
    /// The network addresses to reach the `Seed` on. It is common for this to
    /// be a `String` address that can be resolved by [`Seed::resolve`] to a
    /// list of [`SocketAddr`].
    pub addrs: Addrs,
}

impl<Addrs: fmt::Display> fmt::Display for Seed<Addrs> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.peer, self.addrs)
    }
}

impl FromStr for Seed<String> {
    type Err = error::Parse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once("@") {
            None => Err(error::Parse::Malformed),
            Some((peer, addrs)) => Ok(Self {
                peer: peer.parse()?,
                addrs: addrs.to_string(),
            }),
        }
    }
}

impl<T> Seed<T> {
    /// Resolve the `Seed`'s address by calling [`tokio::net::lookup_host`].
    ///
    /// # Errors
    ///
    /// If the addresses returned by `lookup_host` are empty, this will result
    /// in an [`error::Resolve::DnsLookupFailed`].
    pub async fn resolve(&self) -> Result<Seed<Vec<SocketAddr>>, error::Resolve>
    where
        T: Clone + ToSocketAddrs,
    {
        let addrs = lookup_host(self.addrs.clone()).await?.collect::<Vec<_>>();
        if !addrs.is_empty() {
            Ok(Seed {
                peer: self.peer,
                addrs,
            })
        } else {
            Err(error::Resolve::DnsLookupFailed(self.peer))
        }
    }
}

/// A list of [`Seed`]s that have been resolved.
pub struct Seeds(pub Vec<Seed<Vec<SocketAddr>>>);

impl Seeds {
    /// Build up the list of [`Seed`]s, resolving their network addresses.
    pub async fn resolve(
        seeds: impl ExactSizeIterator<Item = &Seed<String>>,
    ) -> Result<Self, error::Resolve> {
        let mut resolved = Vec::with_capacity(seeds.len());

        for seed in seeds {
            resolved.push(seed.resolve().await?);
        }

        Ok(Self(resolved))
    }
}

impl TryFrom<Seeds> for discovery::Static {
    type Error = io::Error;

    fn try_from(seeds: Seeds) -> Result<Self, Self::Error> {
        discovery::Static::resolve(
            seeds
                .0
                .iter()
                .map(|seed| (seed.peer, seed.addrs.as_slice())),
        )
    }
}

pub mod error {
    use std::io;
    use thiserror::Error;

    use librad::{crypto::peer, PeerId};

    #[derive(Debug, Error)]
    pub enum Parse {
        #[error("entry should be of the form `<peer id> <host>`")]
        Malformed,

        #[error(transparent)]
        Peer(#[from] peer::conversion::Error),
    }

    #[derive(Debug, Error)]
    pub enum Resolve {
        #[error("no address could be resolved for `{0}`")]
        DnsLookupFailed(PeerId),

        #[error(transparent)]
        Io(#[from] io::Error),
    }
}
