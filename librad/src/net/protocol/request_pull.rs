// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;

use link_async::Spawner;
use thiserror::Error;

#[cfg(feature = "replication-v3")]
use crate::{
    git::storage::{PoolError, ReadOnlyStorage as _},
    net::quic,
};

use crate::{
    data::NonEmptyOrderedSet,
    git::{
        storage::{self},
        Urn,
    },
    net::replication,
    paths::Paths,
    PeerId,
};

pub mod auth;
pub use auth::Auth;
mod rpc;
pub use rpc::{Error, Progress, Ref, Request, Response, Success};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub peers: BTreeSet<PeerId>,
    pub urns: BTreeSet<Urn>,
}

impl From<Config> for auth::ProtocolAuth {
    fn from(Config { peers, urns }: Config) -> Self {
        match (
            NonEmptyOrderedSet::from_maybe_empty(peers),
            NonEmptyOrderedSet::from_maybe_empty(urns),
        ) {
            (None, None) => Self::AllowAll(auth::AllowAll),
            (Some(peers), None) => Self::Configured(auth::Configured::Peers(peers)),
            (None, Some(urns)) => Self::Configured(auth::Configured::Urns(urns)),
            (Some(peers), Some(urns)) => Self::Configured(auth::Configured::Both { peers, urns }),
        }
    }
}

#[derive(Clone)]
pub struct State<S, A> {
    storage: S,
    // unused unless replication-v3
    #[allow(dead_code)]
    paths: Paths,
    authorization: A,
}

impl<S, A: Auth> State<S, A> {
    pub fn new(storage: S, paths: Paths, authorization: A) -> Self {
        Self {
            storage,
            paths,
            authorization,
        }
    }

    pub fn is_authorized(&self, peer: &PeerId, urn: &Urn) -> bool {
        self.authorization.is_authorized(peer, urn)
    }
}

#[cfg(not(feature = "replication-v3"))]
pub(in crate::net::protocol) struct PeerAddr {
    pub peer: PeerId,
    pub addr: std::net::SocketAddr,
}

#[cfg(feature = "replication-v3")]
pub(in crate::net::protocol) struct Connection(pub quic::Connection);

pub(in crate::net::protocol) struct Replicate<Conn> {
    pub conn: Conn,
}

pub(in crate::net::protocol) enum SomeReplicate {
    #[cfg(not(feature = "replication-v3"))]
    V2(Replicate<PeerAddr>),
    #[cfg(feature = "replication-v3")]
    V3(Replicate<Connection>),
}

pub(in crate::net::protocol) mod error {
    use super::*;

    #[derive(Debug, Error)]
    pub enum Replicate {
        #[error(transparent)]
        Replication(#[from] replication::error::Replicate),

        // v3 errors
        #[cfg(feature = "replication-v3")]
        #[error("internal error: could not get handle to storage")]
        Pool(#[from] PoolError),
        #[cfg(feature = "replication-v3")]
        #[error("internal error: could not intialise storage")]
        Init(#[from] replication::error::Init),
        #[cfg(feature = "replication-v3")]
        #[error("internal error: failed to look up symbolic-ref target")]
        Read(#[from] storage::read::Error),
    }
}

impl<S, A> State<S, A>
where
    S: storage::Pooled<storage::Storage> + Send + Sync + 'static,
{
    #[cfg(not(feature = "replication-v3"))]
    pub(in crate::net::protocol) async fn replicate(
        &self,
        spawner: &Spawner,
        urn: Urn,
        Replicate {
            conn: PeerAddr { peer, addr },
        }: Replicate<PeerAddr>,
    ) -> Result<Vec<Ref>, error::Replicate> {
        let repl = replication::Replication::new(replication::Config::default());

        let succ = repl
            .replicate(spawner, &self.storage, (peer, vec![addr]), urn, None)
            .await?;
        Ok(succ
            .updated_tips
            .into_iter()
            .map(|(name, oid)| Ref { name, oid })
            .collect())
    }

    #[cfg(feature = "replication-v3")]
    pub(in crate::net::protocol) async fn replicate(
        &self,
        spawner: &Spawner,
        urn: Urn,
        Replicate {
            conn: Connection(conn),
        }: Replicate<Connection>,
    ) -> Result<Vec<Ref>, error::Replicate> {
        use link_replication::Updated;

        let repl = replication::Replication::new(&self.paths, replication::Config::default())?;
        let storage = self.storage.get().await?;
        let succ = repl.replicate(spawner, storage, conn, urn, None).await?;

        let storage = self.storage.get().await?;
        Ok(succ
            .updated_refs()
            .iter()
            .map(|up| match up {
                Updated::Direct { name, target } => Ok(Ref {
                    name: name.into(),
                    oid: (*target).into(),
                }),
                Updated::Symbolic { name, target } => {
                    (*storage).reference_oid(target).map(|oid| Ref {
                        name: name.into(),
                        oid,
                    })
                },
            })
            .collect::<Result<_, _>>()?)
    }
}

pub mod progress {
    use super::*;

    pub fn replicating(peer: &PeerId, urn: &Urn) -> Progress {
        Progress {
            message: format!("Starting replication from `{}` for `{}`", peer, urn),
        }
    }

    pub fn authorizing(peer: &PeerId, urn: &Urn) -> Progress {
        Progress {
            message: format!("Authorizing `{}` and `{}`", peer, urn),
        }
    }
}
