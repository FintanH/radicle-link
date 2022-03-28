use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use librad::{
    crypto::Signer,
    git::{identities::local::LocalIdentity, Urn},
    net::{
        peer::{config, error, Config, Peer},
        protocol,
        replication,
    },
    paths::Paths,
    PeerId,
};

/// A [`Peer`] that does not bind to its socket, and thus is not connected to
/// the network.
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
                // unused
                listen_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
                advertised_addrs: None,
                membership: Default::default(),
                network: Default::default(),
                replication: replication::Config::default(),
                rate_limits: Default::default(),
            },
            storage: config::Storage::default(),
        };
        Peer::new(config).map(Self)
    }

    pub async fn replicate(
        &self,
        from: impl Into<(PeerId, Vec<SocketAddr>)>,
        urn: Urn,
        whoami: Option<LocalIdentity>,
    ) -> Result<replication::Success, error::Replicate> {
        self.0.replicate(from, urn, whoami).await
    }
}
