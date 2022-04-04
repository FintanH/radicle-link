// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{net::SocketAddr, sync::Arc};

use either::Either;
use futures::{Stream, StreamExt, TryFutureExt};
use identities::Xor;
use link_async::Spawner;

use crate::{
    git::{self, identities::local::LocalIdentity, Urn},
    net::{
        connection::{CloseReason, RemoteAddr as _, RemotePeer},
        protocol::{self, interrogation, io, request_pull, PeerAdvertisement},
        quic,
        replication::{self, Replication},
        upgrade,
    },
    paths::Paths,
    PeerId,
};

#[derive(Clone)]
pub struct Client<Endpoint: Clone + Send + Sync> {
    local_id: PeerId,
    spawner: Arc<Spawner>,
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
    #[error("unable to obtain connection to {0}")]
    pub struct NoConnection(pub PeerId);
}

#[async_trait]
pub trait Connection {
    async fn connect(
        &self,
        remote: impl Into<(PeerId, Vec<SocketAddr>)>,
    ) -> Option<quic::Connection>;
}

impl<E> Client<E>
where
    E: Connection + Clone + Send + Sync,
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
            let from = from.into();
            let remote_peer = from.0;
            let conn = self
                .endpoint
                .connect(from)
                .await
                .ok_or(error::Replicate::NoConnection(remote_peer))?;
            let store = self.user_store.gpet().await?;
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
        let to = to.into();
        let remote_peer = to.0;
        let conn = self
            .endpoint
            .connect(to)
            .await
            .ok_or(error::NoConnection(remote_peer))?;

        let reply = protocol::io::send::multi_response(
            &conn,
            request_pull::Request { urn },
            request_pull::FRAMED_BUFSIZ,
        )
        .await?
        .map(|i| i.map_err(error::RequestPull::from));
        Ok(reply)
    }

    pub async fn interrogate(
        &self,
        from: impl Into<(PeerId, Vec<SocketAddr>)>,
    ) -> Result<Interrogation, error::NoConnection> {
        let from = from.into();
        let remote_peer = from.0;
        let conn = self
            .endpoint
            .connect(from)
            .await
            .ok_or(error::NoConnection(remote_peer))?;

        Ok(Interrogation {
            peer: remote_peer,
            conn,
        })
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
pub(in crate::net::protocol) async fn incoming<I>(
    spawner: Arc<Spawner>,
    paths: Arc<Paths>,
    streams: quic::IncomingStreams<I>,
) where
    I: Stream<Item = quic::Result<Either<quic::BidiStream, quic::RecvStream>>> + Unpin,
{
    use Either::{Left, Right};

    let streams = streams.fuse();
    futures::pin_mut!(streams);
    loop {
        match streams.next().await {
            None => {
                tracing::info!("connection lost");
                break;
            },
            Some(stream) => {
                tracing::info!("new ingress stream");
                match stream {
                    Ok(s) => match s {
                        Left(bidi) => spawner.spawn(handle_bidi(paths.clone(), bidi)).detach(),
                        Right(uni) => spawner.spawn(handle_uni(uni)).detach(),
                    },
                    Err(e) => {
                        tracing::warn!(err = ?e, "ingress stream error");
                        break;
                    },
                }
            },
        }
    }
}

pub(super) async fn handle_bidi(paths: Arc<Paths>, stream: quic::BidiStream) {
    use upgrade::SomeUpgraded::*;

    match upgrade::with_upgraded(stream).await {
        Err(upgrade::Error { stream, source }) => {
            tracing::warn!(err = ?source, "invalid upgrade");
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

pub(super) async fn handle_uni(stream: quic::RecvStream) {
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
