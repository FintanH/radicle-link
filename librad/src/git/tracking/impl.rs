// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::TryFrom as _;

use thiserror::Error;

use git_ext::{is_not_found_err, Oid, RefLike};
use link_canonical::{json::Value, Canonical as _, Cstring};
use link_crypto::PeerId;
use link_identities::git::Urn;
use link_tracking::Tracking;
use std_ext::result::ResultExt as _;

use crate::{
    git::{
        storage::{self, glob, read::ReadOnlyStorage as _, Storage},
        types::Namespace,
    },
    reflike,
};

use super::config::{self, Config};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Git(#[from] git2::Error),

    #[error("can't track oneself")]
    SelfReferential,

    #[error(transparent)]
    Store(#[from] storage::Error),

    #[error(transparent)]
    Tracked(#[from] tracked::Error),
}

pub type Tracked = link_tracking::Tracked<Oid, Config>;

impl Tracking<Oid, Cstring, Cstring> for Storage {
    type Error = Error;
    type Config = Config;

    fn track(
        &self,
        urn: &Urn,
        peer: Option<PeerId>,
        config: Option<Config>,
    ) -> Result<bool, Self::Error> {
        let local_peer = self.peer_id();

        if let Some(peer) = peer {
            if &peer == local_peer {
                return Err(Error::SelfReferential);
            }
        }

        let refl = RefLike::from(Reference::new(urn, peer));
        let raw = self.as_raw();

        if raw
            .find_reference(refl.as_str())
            .and(Ok(true))
            .or_matches::<git2::Error, _, _>(is_not_found_err, || Ok(false))?
        {
            return Ok(false);
        }

        let config = config.unwrap_or_default();
        // NOTE: unwrap is safe because error side is void
        let blob = raw.blob(&config.canonical_form().unwrap())?;
        let mut builder = raw.treebuilder(None)?;
        builder.insert(blob.to_string(), blob, 0o100_644)?;
        builder.write()?;

        Ok(true)
    }

    fn untrack(&self, urn: &Urn, peer: PeerId) -> Result<bool, Self::Error> {
        let refl = RefLike::from(Reference::new(urn, peer));
        let backend = self.as_raw();

        if backend
            .find_reference(refl.as_str())
            .and(Ok(true))
            .or_matches::<git2::Error, _, _>(is_not_found_err, || Ok(false))?
        {
            return Ok(false);
        }

        let mut tracking = backend.find_reference(refl.as_str())?;
        tracking.delete()?;

        // Prune all remote branches
        let prune = self.references_glob(glob::RefspecMatcher::from(
            reflike!("refs/namespaces")
                .join(urn)
                .join(reflike!("refs/remotes"))
                .join(peer)
                .with_pattern_suffix(refspec_pattern!("*")),
        ))?;

        for branch in prune {
            branch?.delete()?;
        }

        Ok(true)
    }

    fn update(&self, _urn: &Urn, _peer: PeerId, _config: Config) -> Result<(), Self::Error> {
        todo!()
    }

    fn tracked(&self, filter_by: Option<&Urn>) -> Result<Vec<Tracked>, Self::Error> {
        let remotes = reflike!("refs/rad/remotes");
        let glob = match filter_by {
            None => remotes.with_pattern_suffix(refspec_pattern!("*")),
            Some(urn) => remotes.join(urn).with_pattern_suffix(refspec_pattern!("*")),
        };
        let tracked = self.references_glob(glob::RefspecMatcher::from(glob))?;
        tracked
            .map(|r| {
                r.map_err(Error::from)
                    .and_then(|r| tracked_from_reference(r).map_err(Error::from))
            })
            .collect::<Result<_, _>>()
    }

    fn get(&self, urn: &Urn, peer: Option<PeerId>) -> Result<Option<Tracked>, Self::Error> {
        let refl = RefLike::from(Reference::new(urn, peer));
        let backend = self.as_raw();

        let reference = match backend
            .find_reference(refl.as_str())
            .map(Some)
            .or_matches::<git2::Error, _, _>(is_not_found_err, || Ok(None))?
        {
            None => return Ok(None),
            Some(r) => r,
        };

        tracked_from_reference(reference)
            .map_err(Error::from)
            .map(Some)
    }
}

pub enum Remote {
    Default,
    Peer(PeerId),
}

impl Default for Remote {
    fn default() -> Self {
        Self::Default
    }
}

impl From<Remote> for RefLike {
    fn from(refl: Remote) -> Self {
        match refl {
            Remote::Default => reflike!("default"),
            Remote::Peer(peer) => peer.into(),
        }
    }
}

pub struct Reference {
    pub remote: Remote,
    pub namespace: Namespace<Oid>,
}

impl Reference {
    pub fn new<P>(urn: &Urn, peer: P) -> Self
    where
        P: Into<Option<PeerId>>,
    {
        Self {
            remote: peer.into().map(Remote::Peer).unwrap_or_default(),
            namespace: Namespace::from(urn),
        }
    }
}

impl From<Reference> for RefLike {
    fn from(refl: Reference) -> Self {
        reflike!("refs/rad/remotes")
            .join(refl.namespace)
            .join(RefLike::from(refl.remote))
    }
}

pub mod tracked {
    use super::*;

    use git_ext::FromMultihashError;
    use link_identities::urn;

    #[non_exhaustive]
    #[derive(Debug, Error)]
    pub enum Error {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error("could not determine name of reference")]
        MissingName,
        #[error("expected reference name to end in a peer identifier or `default`")]
        MissingSuffix,
        #[error("expected peer identifier or `default`, but found: {0}")]
        UnknownSuffix(String),
        #[error("expected namespace")]
        MissingNamespace,
        #[error(transparent)]
        Urn(#[from] urn::error::DecodeId<FromMultihashError>),
        #[error(transparent)]
        Config(#[from] config::Error),
        #[error("failed to parse Canonical JSON: {0}")]
        Cjson(String),
    }
}

fn tracked_from_reference(r: git2::Reference<'_>) -> Result<Tracked, tracked::Error> {
    use tracked::Error;

    let name = r.name().ok_or(Error::MissingName)?;

    let mut components = name.rsplit('/');

    let remote = if let Some(suffix) = components.next() {
        match suffix {
            "default" => Remote::Default,
            _ => suffix
                .parse()
                .map(Remote::Peer)
                .map_err(|_| Error::UnknownSuffix(suffix.to_string()))?,
        }
    } else {
        return Err(Error::MissingSuffix);
    };

    let urn = components
        .next()
        .ok_or(Error::MissingNamespace)
        .and_then(|urn| Urn::try_from_id(urn).map_err(Error::from))?;

    // TODO(finto): catch error
    let blob = r.peel_to_blob()?;
    let config = std::str::from_utf8(blob.content())
        .unwrap()
        .parse::<Value>()
        .map_err(Error::Cjson)
        .and_then(|val| Config::try_from(&val).map_err(Error::from))?;

    Ok(match remote {
        Remote::Default => Tracked::Default { urn, config },
        Remote::Peer(peer) => Tracked::Peer { urn, peer, config },
    })
}
