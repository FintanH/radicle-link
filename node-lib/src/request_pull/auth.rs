// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;

use librad::{data::NonEmptyOrderedSet, git::Urn, net::protocol::request_pull::Auth, PeerId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub peers: BTreeSet<PeerId>,
    pub urns: BTreeSet<Urn>,
}

impl From<Config> for ProtocolAuth {
    fn from(Config { peers, urns }: Config) -> Self {
        match (
            NonEmptyOrderedSet::from_maybe_empty(peers),
            NonEmptyOrderedSet::from_maybe_empty(urns),
        ) {
            (None, None) => Self::AllowAll(AllowAll),
            (Some(peers), None) => Self::Configured(Configured::Peers(peers)),
            (None, Some(urns)) => Self::Configured(Configured::Urns(urns)),
            (Some(peers), Some(urns)) => Self::Configured(Configured::Both { peers, urns }),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ProtocolAuth {
    AllowAll(AllowAll),
    Configured(Configured),
}

impl Auth for ProtocolAuth {
    fn is_authorized(&self, peer: &PeerId, urn: &Urn) -> bool {
        match self {
            Self::AllowAll(x) => x.is_authorized(peer, urn),
            Self::Configured(x) => x.is_authorized(peer, urn),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Configured {
    Peers(NonEmptyOrderedSet<PeerId>),
    Urns(NonEmptyOrderedSet<Urn>),
    Both {
        peers: NonEmptyOrderedSet<PeerId>,
        urns: NonEmptyOrderedSet<Urn>,
    },
}

impl Auth for Configured {
    fn is_authorized(&self, peer: &PeerId, urn: &Urn) -> bool {
        match self {
            Self::Peers(peers) => peers.contains(peer),
            Self::Urns(urns) => urns.contains(urn),
            Self::Both { peers, urns } => peers.contains(peer) && urns.contains(urn),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AllowAll;

impl Auth for AllowAll {
    fn is_authorized(&self, _: &PeerId, _: &Urn) -> bool {
        true
    }
}
