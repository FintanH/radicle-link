// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{future::Future as _, net::SocketAddr, pin, sync::Arc, task};

use crypto::Signer;
use either::Either;
use futures::{Stream, StreamExt, TryFutureExt};
use identities::Xor;
use link_async::{Spawner, Task};

use crate::{
    git::{self, identities::local::LocalIdentity, Urn},
    net::{
        connection::{CloseReason, RemoteAddr as _, RemotePeer},
        protocol::{self, interrogation, io, request_pull, PeerAdvertisement},
        quic::{self, ConnectPeer},
        replication::{self, Replication},
        upgrade,
        Network,
    },
    paths::Paths,
    PeerId,
};

#[derive(Clone)]
pub struct Config<Signer> {
    pub signer: Signer,
    pub paths: Paths,
    pub replication: replication::Config,
    pub user_storage: UserStorage,
    pub network: Network,
}

#[derive(Clone, Debug)]
pub struct UserStorage {
    pool_size: usize,
}

impl Default for UserStorage {
    fn default() -> Self {
        Self {
            pool_size: num_cpus::get_physical(),
        }
    }
}

#[derive(Clone)]
pub struct Client<Signer, Endpoint: Clone + Send + Sync> {
    config: Config<Signer>,
    local_id: PeerId,
    spawner: Arc<Spawner>,
    paths: Arc<Paths>,
    endpoint: Endpoint,
    repl: Replication,
    user_store: git::storage::Pool<git::storage::Storage>,
}

pub mod error {
    use thiserror::Error;

    use crate::{
        git::storage,
        net::{
            protocol::{self, interrogation},
            quic,
            replication,
        },
        PeerId,
    };

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Init {
        #[error(transparent)]
        Quic(#[from] quic::error::Error),

        #[error("no async context found, try calling `.enter()` on the runtime")]
        Runtime,

        #[error(transparent)]
        Storage(#[from] storage::error::Init),

        #[cfg(feature = "replication-v3")]
        #[error(transparent)]
        Replication(#[from] replication::error::Init),
    }

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Interrogation {
        #[error("no response from {0}")]
        NoResponse(PeerId),

        #[error("error response: {0:?}")]
        ErrorResponse(interrogation::Error),

        #[error("invalid response")]
        InvalidResponse,

        #[error(transparent)]
        Rpc(#[from] Box<protocol::error::Rpc<quic::BidiStream>>),
    }

    impl From<protocol::error::Rpc<quic::BidiStream>> for Interrogation {
        fn from(e: protocol::error::Rpc<quic::BidiStream>) -> Self {
            Self::Rpc(Box::new(e))
        }
    }

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum RequestPull {
        #[error(transparent)]
        Incoming(#[from] Incoming),

        #[error(transparent)]
        NoConnection(#[from] NoConnection),

        #[error(transparent)]
        Rpc(#[from] Box<protocol::error::Rpc<quic::BidiStream>>),
    }

    impl From<protocol::error::Rpc<quic::BidiStream>> for RequestPull {
        fn from(e: protocol::error::Rpc<quic::BidiStream>) -> Self {
            Self::Rpc(Box::new(e))
        }
    }

    #[derive(Debug, Error)]
    pub enum Replicate {
        #[error("no connection to {0}")]
        NoConnection(PeerId),

        #[error("failed to borrow storage from pool")]
        Pool(#[from] storage::PoolError),

        #[error(transparent)]
        Replicate(#[from] replication::error::Replicate),
    }

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Storage {
        #[error(transparent)]
        Storage(#[from] storage::Error),

        #[error(transparent)]
        Pool(storage::PoolError),
    }

    impl From<storage::PoolError> for Storage {
        fn from(e: storage::PoolError) -> Self {
            Self::Pool(e)
        }
    }

    #[derive(Debug, Error)]
    #[error("unable to obtain connection to {0}")]
    pub struct NoConnection(pub PeerId);

    #[derive(Debug, Error)]
    pub enum Incoming {
        #[error(transparent)]
        Quic(#[from] quic::error::Error),
        #[error("expected bidirectional connection, but found a unidirectional connection")]
        Uni,
        #[error("connection lost")]
        ConnectionLost,
    }
}

struct RequestPull<S> {
    responses: S,
    replicate: Option<Task<()>>,
}

impl<S> Stream for RequestPull<S>
where
    S: Stream<Item = Result<request_pull::Response, error::RequestPull>> + Unpin,
{
    type Item = Result<request_pull::Response, error::RequestPull>;

    fn poll_next(
        mut self: pin::Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        match &mut self.replicate {
            Some(replicate) => {
                futures::pin_mut!(replicate);
                match replicate.poll(cx) {
                    task::Poll::Ready(Ok(())) => {
                        tracing::trace!("request-pull replication task completed");
                        self.replicate = None;
                    },
                    task::Poll::Ready(Err(e)) => {
                        // TODO(finto): propagate panic
                        tracing::warn!(err = %e, "request-pull replication task failed")
                    },
                    task::Poll::Pending => {},
                }
            },
            None => {},
        }
        self.responses.poll_next_unpin(cx)
    }
}

impl<S> Client<S, protocol::Endpointless>
where
    S: Signer + Clone,
{
    pub async fn new(config: Config<S>) -> Result<Self, error::Init> {
        let local_id = PeerId::from_signer(&config.signer);
        let spawner = Spawner::from_current()
            .map(Arc::new)
            .ok_or(error::Init::Runtime)?;
        let user_store = git::storage::Pool::new(
            git::storage::pool::ReadWriteConfig::new(
                config.paths.clone(),
                config.signer.clone(),
                git::storage::pool::Initialised::no(),
            ),
            config.user_storage.pool_size,
        );

        #[cfg(feature = "replication-v3")]
        let repl = Replication::new(&config.paths, config.replication)?;
        #[cfg(not(feature = "replication-v3"))]
        let repl = Replication::new(config.replication);
        let endpoint =
            protocol::Endpointless::new(config.signer.clone(), config.network.clone()).await?;
        let paths = config.paths.clone();
        Ok(Self {
            config,
            local_id,
            spawner,
            paths: Arc::new(paths),
            endpoint,
            repl,
            user_store,
        })
    }
}

impl<S, E> Client<S, E>
where
    S: Signer + Clone,
    E: ConnectPeer + Clone + Send + Sync,
{
    pub fn peer_id(&self) -> PeerId {
        self.local_id
    }

    pub async fn replicate(
        &self,
        from: impl Into<(PeerId, Vec<SocketAddr>)>,
        urn: Urn,
        whoami: Option<LocalIdentity>,
    ) -> Result<replication::Success, error::Replicate> {
        #[cfg(feature = "replication-v3")]
        {
            // TODO: errors
            let (remote_peer, addrs) = from.into();
            let (conn, _) = io::connect(&self.endpoint, remote_peer, addrs)
                .await
                .ok_or(error::Replicate::NoConnection(remote_peer))?;
            let store = self.user_store.get().await?;
            self.repl
                .replicate(&self.spawner, store, conn, urn, whoami)
                .err_into()
                .await
        }
        #[cfg(not(feature = "replication-v3"))]
        {
            self.repl
                .replicate(&self.spawner, &self.user_store, from, urn, whoami)
                .err_into()
                .await
        }
    }

    pub async fn request_pull(
        &self,
        to: impl Into<(PeerId, Vec<SocketAddr>)>,
        urn: Urn,
    ) -> Result<
        impl Stream<Item = Result<request_pull::Response, error::RequestPull>>,
        error::RequestPull,
    > {
        let (remote_peer, addrs) = to.into();
        let (conn, incoming) = io::connect(&self.endpoint, remote_peer, addrs)
            .await
            .ok_or(error::NoConnection(remote_peer))?;

        let reply = protocol::io::send::multi_response(
            &conn,
            request_pull::Request { urn },
            request_pull::FRAMED_BUFSIZ,
        )
        .await?
        .map(|i| i.map_err(error::RequestPull::from));

        let replicate = incoming_git(self.spawner.clone(), self.paths.clone(), incoming).await?;
        Ok(RequestPull {
            responses: reply,
            replicate: Some(replicate),
        })
    }

    pub async fn interrogate(
        &self,
        from: impl Into<(PeerId, Vec<SocketAddr>)>,
    ) -> Result<Interrogation, error::NoConnection> {
        let (remote_peer, addrs) = from.into();
        let (conn, _) = io::connect(&self.endpoint, remote_peer, addrs)
            .await
            .ok_or(error::NoConnection(remote_peer))?;

        Ok(Interrogation {
            peer: remote_peer,
            conn,
        })
    }

    /// Borrow a [`git::storage::Storage`] from the pool, and run a blocking
    /// computation on it.
    pub async fn using_storage<F, T>(&self, blocking: F) -> Result<T, error::Storage>
    where
        F: FnOnce(&git::storage::Storage) -> T + Send + 'static,
        T: Send + 'static,
    {
        let storage = self.user_store.get().await?;
        Ok(self.spawner.blocking(move || blocking(&storage)).await)
    }
}

/// Dispatch incoming streams.
///
/// # Panics
///
/// Panics if one of the tasks spawned by this function panics.
#[tracing::instrument(
    skip(spawner, streams),
    fields(
        remote_id = %streams.remote_peer_id(),
        remote_addr = %streams.remote_addr()
    )
)]
pub(in crate::net::protocol) async fn incoming_git<I>(
    spawner: Arc<Spawner>,
    paths: Arc<Paths>,
    streams: quic::IncomingStreams<I>,
) -> Result<Task<()>, error::Incoming>
where
    I: Stream<Item = quic::Result<Either<quic::BidiStream, quic::RecvStream>>> + Unpin,
{
    use Either::{Left, Right};

    let streams = streams.fuse();
    futures::pin_mut!(streams);
    match streams.next().await {
        None => {
            tracing::info!("connection lost");
            Err(error::Incoming::ConnectionLost)
        },
        Some(stream) => {
            tracing::info!("new ingress stream");
            match stream {
                Ok(s) => match s {
                    Left(bidi) => Ok(spawner.spawn(handle_bidi(paths.clone(), bidi))),
                    Right(uni) => {
                        handle_uni(uni);
                        Err(error::Incoming::Uni)
                    },
                },
                Err(e) => {
                    tracing::warn!(err = ?e, "ingress stream error");
                    Err(e.into())
                },
            }
        },
    }
}

pub(super) async fn handle_bidi(paths: Arc<Paths>, stream: quic::BidiStream) {
    use upgrade::SomeUpgraded::*;

    match upgrade::with_upgraded(stream).await {
        Err(upgrade::Error { stream, source }) => {
            tracing::warn!(err = ?source, "invalid upgrade");
            // TODO(finto): consider returning an error
            stream.close(CloseReason::InvalidUpgrade)
        },

        Ok(Git(up)) => io::recv::git(&paths, up).await,
        Ok(Gossip(up)) => deny_bidi(up.into_stream(), "gossip"),
        Ok(Membership(up)) => deny_bidi(up.into_stream(), "membership"),
        Ok(Interrogation(up)) => deny_bidi(up.into_stream(), "interrogation"),
        Ok(RequestPull(up)) => deny_bidi(up.into_stream(), "request-pull"),
    }
}

fn deny_bidi(stream: quic::BidiStream, kind: &str) {
    tracing::warn!("non-git bidirectional {} requested", kind);
    stream.close(CloseReason::InvalidUpgrade)
}

pub(super) fn handle_uni(stream: quic::RecvStream) {
    stream.close(CloseReason::InvalidUpgrade)
}

pub struct Interrogation {
    peer: PeerId,
    conn: quic::Connection,
}

impl Interrogation {
    /// Ask the interrogated peer to send its [`PeerAdvertisement`].
    pub async fn peer_advertisement(
        &self,
    ) -> Result<PeerAdvertisement<SocketAddr>, error::Interrogation> {
        use interrogation::{Request, Response};

        self.request(Request::GetAdvertisement)
            .await
            .and_then(|resp| match resp {
                Response::Advertisement(ad) => Ok(ad),
                Response::Error(e) => Err(error::Interrogation::ErrorResponse(e)),
                _ => Err(error::Interrogation::InvalidResponse),
            })
    }

    /// Ask the interrogated peer to send back the [`SocketAddr`] the local peer
    /// appears to have.
    pub async fn echo_addr(&self) -> Result<SocketAddr, error::Interrogation> {
        use interrogation::{Request, Response};

        self.request(Request::EchoAddr)
            .await
            .and_then(|resp| match resp {
                Response::YourAddr(ad) => Ok(ad),
                Response::Error(e) => Err(error::Interrogation::ErrorResponse(e)),
                _ => Err(error::Interrogation::InvalidResponse),
            })
    }

    /// Ask the interrogated peer to send the complete list of URNs it has.
    ///
    /// The response is compactly encoded as an [`Xor`] filter, with a very
    /// small false positive probability.
    pub async fn urns(&self) -> Result<Xor, error::Interrogation> {
        use interrogation::{Request, Response};

        self.request(Request::GetUrns)
            .await
            .and_then(|resp| match resp {
                Response::Urns(urns) => Ok(urns.into_owned()),
                Response::Error(e) => Err(error::Interrogation::ErrorResponse(e)),
                _ => Err(error::Interrogation::InvalidResponse),
            })
    }

    async fn request(
        &self,
        request: interrogation::Request,
    ) -> Result<interrogation::Response<'static, SocketAddr>, error::Interrogation> {
        match io::send::single_response(&self.conn, request, interrogation::FRAMED_BUFSIZ).await {
            Err(e) => Err(e.into()),
            Ok(resp) => resp.ok_or(error::Interrogation::NoResponse(self.peer)),
        }
    }
}
