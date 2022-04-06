// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{net::SocketAddr, sync::Arc};

use crypto::Signer;

use futures::{Stream, StreamExt, TryFutureExt};

use link_async::Spawner;

use crate::{
    git::{self, identities::local::LocalIdentity, Urn},
    net::{
        protocol::{self, io},
        quic::ConnectPeer,
        replication::{self, Replication},
    },
    paths::Paths,
    PeerId,
};

pub mod config;
pub use config::Config;
pub mod error;

mod interrogation;
pub use interrogation::Interrogation;
mod request_pull;
use request_pull::RequestPull;
mod streams;

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

impl<S> Client<S, protocol::SendOnly>
where
    S: Signer + Clone,
{
    pub async fn new(config: Config<S>) -> Result<Self, error::Init> {
        let paths = config.paths.clone();
        let local_id = PeerId::from_signer(&config.signer);
        let spawner = Spawner::from_current()
            .map(Arc::new)
            .ok_or(error::Init::Runtime)?;
        let user_store = git::storage::Pool::new(
            git::storage::pool::ReadWriteConfig::new(
                paths.clone(),
                config.signer.clone(),
                git::storage::pool::Initialised::no(),
            ),
            config.user_storage.pool_size,
        );
        let endpoint =
            protocol::SendOnly::new(config.signer.clone(), config.network.clone()).await?;
        #[cfg(feature = "replication-v3")]
        let repl = Replication::new(&paths, config.replication)?;
        #[cfg(not(feature = "replication-v3"))]
        let repl = Replication::new(config.replication);

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
    pub fn paths(&self) -> &Paths {
        &self.config.paths
    }

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
        impl Stream<Item = Result<protocol::request_pull::Response, error::RequestPull>>,
        error::RequestPull,
    > {
        let (remote_peer, addrs) = to.into();
        let (conn, incoming) = io::connect(&self.endpoint, remote_peer, addrs)
            .await
            .ok_or(error::NoConnection(remote_peer))?;

        let reply = protocol::io::send::multi_response(
            &conn,
            protocol::request_pull::Request { urn },
            protocol::request_pull::FRAMED_BUFSIZ,
        )
        .await?
        .map(|i| i.map_err(error::RequestPull::from));

        let replicate = streams::git(self.spawner.clone(), self.paths.clone(), incoming).await?;
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
