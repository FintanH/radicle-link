// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

pub use crate::identities::git::Urn;

mod odb;
mod refdb;

pub use link_tracking::{
    git::{
        config::Config,
        tracking::{
            error,
            get,
            is_tracked,
            policy,
            track,
            tracked,
            tracked_peers,
            untrack,
            TrackedEntries,
            TrackedPeers,
        },
    },
    *,
};

pub type Tracked = tracking::Tracked<git_ext::Oid, Config>;
