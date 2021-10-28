#[cfg(feature = "cjson")]
#[macro_use]
extern crate link_canonical;

use link_crypto::PeerId;
use link_identities::Urn;

pub mod config;

pub enum Tracked<R, Config> {
    Default {
        urn: Urn<R>,
        config: Config,
    },
    Peer {
        urn: Urn<R>,
        peer: PeerId,
        config: Config,
    },
}

// TODO(finto): tracked, get, and is_tracked is read-only so could split it out
pub trait Tracking<R> {
    type Error;
    type Config: config::Configure;

    fn track(
        &self,
        urn: &Urn<R>,
        peer: Option<PeerId>,
        config: Option<Self::Config>,
    ) -> Result<bool, Self::Error>;
    fn untrack(&self, urn: &Urn<R>, peer: PeerId) -> Result<bool, Self::Error>;
    fn update(&self, urn: &Urn<R>, peer: PeerId, config: Self::Config) -> Result<(), Self::Error>;
    fn tracked(
        &self,
        filter_by: Option<&Urn<R>>,
    ) -> Result<Vec<Tracked<R, Self::Config>>, Self::Error>;
    fn get(
        &self,
        urn: &Urn<R>,
        peer: Option<PeerId>,
    ) -> Result<Option<Tracked<R, Self::Config>>, Self::Error>;
    fn is_tracked(&self, urn: &Urn<R>, peer: Option<PeerId>) -> Result<bool, Self::Error> {
        Ok(self.get(urn, peer)?.is_some())
    }
}

pub trait DefaultKey<R> {
    type Key;

    fn key(urn: &Urn<R>) -> Self::Key;
}
