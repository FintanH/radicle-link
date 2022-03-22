// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use link_async::Spawner;
use thiserror::Error;

use crate::{
    git::{storage, storage::PoolError, Urn},
    net::{quic, replication},
    paths::Paths,
    PeerId,
};

mod rpc;
pub use rpc::{Error, Progress, Ref, Request, Response, Success};

pub trait Guard {
    type Error: std::error::Error + Send + Sync + 'static;
    /// The `Output` must implement [`std::fmt::Display`] for reporting back to
    /// the client that made the request in the form of a [`Progress`] message.
    type Output: std::fmt::Display + Send + Sync;

    /// Run any checks and effects required for a request-pull.
    ///
    /// For example, an implementation may want to check if the `peer`
    /// and `urn` are authorized to make the request, and also track
    /// the `peer` for the given `urn`.
    fn guard(&self, peer: &PeerId, urn: &Urn) -> Result<Self::Output, Self::Error>;
}

// storage and paths considered unused under
// #[cfg(not(feature = "replication-v3"))]
#[allow(dead_code)]
/// State for serving request-pull calls.
#[derive(Clone)]
pub struct State<S, G> {
    storage: S,
    paths: Paths,
    guard: G,
}

impl<S, G: Guard> State<S, G> {
    pub fn new(storage: S, paths: Paths, guard: G) -> Self {
        Self {
            storage,
            paths,
            guard,
        }
    }

    pub fn guard(&self, peer: &PeerId, urn: &Urn) -> Result<G::Output, G::Error> {
        self.guard.guard(peer, urn)
    }
}

pub(in crate::net::protocol) mod error {
    use super::*;

    #[derive(Debug, Error)]
    pub enum Replicate {
        #[error(transparent)]
        Replication(#[from] replication::error::Replicate),
        #[error("internal error: could not get handle to storage")]
        Pool(#[from] PoolError),
        #[cfg(feature = "replication-v3")]
        #[error("internal error: could not intialise storage")]
        Init(#[from] replication::error::Init),
        #[error("internal error: failed to look up symbolic-ref target")]
        Read(#[from] storage::read::Error),

        #[cfg(not(feature = "replication-v3"))]
        #[error("request-pull is not implemented without `replication-v3` enabled")]
        Unimplemented,
    }

    pub fn decode_failed() -> Error {
        Error {
            message: "failed to decode request".into(),
        }
    }

    pub fn internal_error() -> Error {
        Error {
            message: "internal error".into(),
        }
    }

    pub fn replication_error(err: Replicate) -> Error {
        Error {
            message: format!("request-pull replication error: {}", err),
        }
    }

    pub fn guard<E: std::error::Error>(e: E) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl<S, G> State<S, G>
where
    S: storage::Pooled<storage::Storage> + Send + Sync + 'static,
{
    /// Run replication and convert the updated tips into [`Ref`]s.
    #[cfg(feature = "replication-v3")]
    pub(in crate::net::protocol) async fn replicate(
        &self,
        spawner: &Spawner,
        urn: Urn,
        conn: quic::Connection,
    ) -> Result<Vec<Ref>, error::Replicate> {
        use crate::git::storage::ReadOnlyStorage as _;
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

    /// # Errors
    ///
    /// request-pull is only implemented for `replication-v3` and will
    /// immediately return [`error::Replicate::Unimplemented`].
    #[cfg(not(feature = "replication-v3"))]
    pub(in crate::net::protocol) async fn replicate(
        &self,
        _: &Spawner,
        _: Urn,
        _: quic::Connection,
    ) -> Result<Vec<Ref>, error::Replicate> {
        Err(error::Replicate::Unimplemented)
    }
}

pub mod progress {
    use super::*;

    pub fn replicating(urn: &Urn) -> Progress {
        Progress {
            message: format!("Starting replication for `{}`", urn),
        }
    }

    pub fn authorizing(urn: &Urn) -> Progress {
        Progress {
            message: format!("Checking if request-pull is allowed for `{}`", urn),
        }
    }

    pub fn guard<T: ToString>(t: T) -> Progress {
        Progress {
            message: t.to_string(),
        }
    }
}
