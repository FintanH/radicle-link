// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use link_crypto::PeerId;
use link_identities::Urn;

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

impl<R, C> Tracked<R, C> {
    pub fn peer_id(&self) -> Option<PeerId> {
        match self {
            Self::Default { .. } => None,
            Self::Peer { peer, .. } => Some(*peer),
        }
    }
}
