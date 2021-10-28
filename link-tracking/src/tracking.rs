// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use link_crypto::PeerId;
use link_identities::Urn;

use crate::config;

pub enum Tracked<R, C> {
    Default {
        urn: Urn<R>,
        config: C,
    },
    Peer {
        urn: Urn<R>,
        peer: PeerId,
        config: C,
    },
}

pub trait Tracking<R, Ty, Id>: Write<R, Ty, Id> + Read<R, Ty, Id> {}

pub trait Write<R, Ty, Id> {
    type Track: std::error::Error + Send + Sync + 'static;
    type Untrack: std::error::Error + Send + Sync + 'static;
    type Update: std::error::Error + Send + Sync + 'static;

    type Config = config::Config<Ty, Id>;

    fn track(
        &self,
        urn: &Urn<R>,
        peer: Option<PeerId>,
        config: Option<Self::Config>,
    ) -> Result<bool, Self::Track>;

    fn untrack(&self, urn: &Urn<R>, peer: PeerId) -> Result<bool, Self::Untrack>;

    fn update(
        &self,
        urn: &Urn<R>,
        peer: PeerId,
        config: Self::Config,
    ) -> Result<bool, Self::Update>;
}

pub trait Read<R, Ty, Id> {
    type Tracked: std::error::Error + Send + Sync + 'static;
    type Get: std::error::Error + Send + Sync + 'static;

    type Config = config::Config<Ty, Id>;

    fn tracked(
        &self,
        filter_by: Option<&Urn<R>>,
    ) -> Result<Vec<Tracked<R, Self::Config>>, Self::Tracked>;

    fn get(
        &self,
        urn: &Urn<R>,
        peer: Option<PeerId>,
    ) -> Result<Option<Tracked<R, Self::Config>>, Self::Get>;

    fn is_tracked(&self, urn: &Urn<R>, peer: Option<PeerId>) -> Result<bool, Self::Get> {
        Ok(self.get(urn, peer)?.is_some())
    }
}
