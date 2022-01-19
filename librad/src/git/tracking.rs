// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

pub use crate::identities::git::Urn;

mod odb;
mod refdb;
pub mod v1;

pub use link_tracking::{
    config,
    git::{
        self,
        config::Config,
        tracking::{
            batch::{self, batch, Action, Applied, Updated},
            default_only,
            error,
            get,
            is_tracked,
            modify,
            policy,
            reference,
            track,
            tracked,
            tracked_peers,
            untrack,
            PreviousError,
            Ref,
            Tracked,
            TrackedEntries,
            TrackedPeers,
        },
    },
};

/// Migration from tracking-v1 to tracking-v2.
///
/// NOTE: This is used in `Storage::open` and will be deprecated once enough
/// time has passed for upstream dependencies to migrate to the latest version.
pub mod migration {
    use std::borrow::Cow;

    use super::*;

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error(transparent)]
        Batch(#[from] error::Batch),
        #[error(transparent)]
        Tracking(#[from] v1::Error),
    }

    pub fn migrate(
        storage: &super::super::Storage,
        urns: impl IntoIterator<Item = Urn>,
    ) -> Result<(), Error> {
        for urn in urns {
            let peers = v1::tracked(storage, &urn)?;
            let config = Config::default();
            let actions = peers.map(|peer| Action::Track {
                peer: Some(peer),
                urn: Cow::from(&urn),
                config: &config,
                policy: policy::Track::MustNotExist,
            });
            let applied = batch(storage, actions)?;
            for update in applied.updates {
                match update {
                    Updated::Tracked { reference } => {
                        let peer = match reference.name.remote.into() {
                            Some(peer) => peer,
                            None => unreachable!("should not track default entry"),
                        };
                        match v1::untrack(storage, &urn, peer) {
                            Ok(_) => tracing::trace!(urn = %urn, peer = %peer, "migrated"),
                            Err(err) => {
                                tracing::trace!(urn = %urn, peer = %peer, reason = %err, "failed to migrate")
                            },
                        }
                    },
                    Updated::Untracked { .. } => {
                        unreachable!("should not untrack during migration")
                    },
                }
            }

            // SAFETY: It could be the case that we successfully tracked for v2 but could
            // not v1::untrack. This would result in a rejection in `applied`, which does
            // not contain the reference for which it attempted to track. We double-check
            // here that the entry is tracked in v2 and attempt to v1::untrack if it is.
            for peer in v1::tracked(storage, &urn)? {
                if let Ok(it_is) = is_tracked(storage, &urn, Some(peer)) {
                    if it_is {
                        match v1::untrack(storage, &urn, peer) {
                            Ok(_) => tracing::trace!(urn = %urn, peer = %peer, "migrated"),
                            Err(err) => {
                                tracing::trace!(urn = %urn, peer = %peer, reason = %err, "failed to migrate")
                            },
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
