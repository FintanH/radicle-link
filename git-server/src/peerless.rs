use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use futures::{future::FutureExt as _, pin_mut, select};
use tokio::{sync::mpsc, time::sleep};

use librad::{
    crypto::Signer,
    git::{identities::local::LocalIdentity, Urn},
    net::{
        peer::{config, error, storage::Storage as PeerStorage, Config, Peer, RequestPull},
        protocol::{self, membership},
        replication,
    },
    paths::Paths,
    PeerId,
};

/// A [`Peer`] that does not bind to its socket, and thus is not connected to
/// the network.
#[derive(Clone)]
pub struct Peerless<S>(Peer<S>);

impl<S> Peerless<S>
where
    S: Signer + Clone,
{
    pub fn new(paths: Paths, signer: S) -> Result<Self, error::Init> {
        let config = Config {
            signer,
            protocol: protocol::Config {
                paths,
                // TODO(finto) do we need a specific listen address?
                listen_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8900)),
                advertised_addrs: None,
                membership: membership::Params {
                    max_active: 1,
                    max_passive: 0,
                    active_random_walk_length: 0,
                    passive_random_walk_length: 0,
                    shuffle_sample_size: 0,
                    ..membership::Params::default()
                },
                network: Default::default(),
                replication: replication::Config::default(),
                rate_limits: Default::default(),
                request_pull: config::DenyAll,
            },
            storage: config::Storage::default(),
        };
        Peer::new(config).map(Self)
    }

    async fn bind(
        &self,
    ) -> Result<protocol::Bound<PeerStorage, config::DenyAll>, protocol::error::Bootstrap> {
        self.0.bind().await
    }

    pub async fn replicate(
        &self,
        from: impl Into<(PeerId, Vec<SocketAddr>)>,
        urn: Urn,
        whoami: Option<LocalIdentity>,
    ) -> Result<replication::Success, error::Replicate> {
        self.0.replicate(from, urn, whoami).await
    }

    pub async fn request_pull(
        &self,
        to: impl Into<(PeerId, Vec<SocketAddr>)>,
        urn: Urn,
    ) -> Result<RequestPull, error::NoConnection> {
        self.0.request_pull(to, urn).await
    }
}

#[tracing::instrument(name = "protocol subroutine", skip(peer, shutdown_rx))]
pub async fn routine<S>(peer: Peerless<S>, mut shutdown_rx: mpsc::Receiver<()>)
where
    S: Signer + Clone,
{
    let shutdown = shutdown_rx.recv().fuse();
    futures::pin_mut!(shutdown);

    loop {
        match peer.bind().await {
            Ok(bound) => {
                let (stop, run) = bound.accept(futures::stream::empty());
                let run = run.fuse();
                pin_mut!(run);

                let res = select! {
                    _ = shutdown => {
                        stop();
                        run.await
                    }
                    res = run => res
                };

                match res {
                    Err(protocol::io::error::Accept::Done) => {
                        tracing::info!("network endpoint shut down");
                        break;
                    },
                    Err(err) => {
                        tracing::error!(?err, "accept error");
                    },
                    Ok(never) => unreachable!("absurd: {}", never),
                }
            },
            Err(err) => {
                tracing::error!(?err, "bind error");

                let sleep = sleep(Duration::from_secs(2)).fuse();
                pin_mut!(sleep);
                select! {
                    _ = sleep => {},
                    _ = shutdown => {
                        break;
                    }
                }
            },
        }
    }
}
