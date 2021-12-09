// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{borrow::Cow, convert::TryFrom, fmt, str::FromStr};

use multihash::Multihash;

use link_crypto::{peer, PeerId};
use link_identities::urn::{HasProtocol, Urn};
use radicle_git_ext::RefLike;

pub fn base() -> RefLike {
    reflike!("refs/rad/remotes")
}

#[derive(Clone, Copy, Debug)]
pub enum Remote {
    Default,
    Peer(PeerId),
}

impl From<Remote> for Option<PeerId> {
    fn from(remote: Remote) -> Self {
        match remote {
            Remote::Default => None,
            Remote::Peer(peer) => Some(peer),
        }
    }
}

impl From<Option<PeerId>> for Remote {
    fn from(peer: Option<PeerId>) -> Self {
        peer.map_or(Self::Default, Self::Peer)
    }
}

impl fmt::Display for Remote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Remote::Default => write!(f, "default"),
            Remote::Peer(peer) => write!(f, "{}", peer),
        }
    }
}

impl FromStr for Remote {
    type Err = peer::conversion::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "default" => Ok(Self::Default),
            _ => s.parse().map(Self::Peer),
        }
    }
}

impl Default for Remote {
    fn default() -> Self {
        Self::Default
    }
}

impl From<&Remote> for RefLike {
    fn from(remote: &Remote) -> Self {
        match remote {
            Remote::Default => reflike!("default"),
            Remote::Peer(peer) => RefLike::from(peer),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReferenceName<'a, R: ToOwned + Clone> {
    pub remote: Remote,
    pub urn: Cow<'a, Urn<R>>,
}

impl<'a, R: ToOwned + Clone> ReferenceName<'a, R> {
    pub fn owned<P>(urn: Urn<R>, peer: P) -> Self
    where
        P: Into<Option<PeerId>>,
    {
        Self {
            remote: peer.into().map(Remote::Peer).unwrap_or_default(),
            urn: Cow::Owned(urn),
        }
    }

    pub fn borrowed<P>(urn: &'a Urn<R>, peer: P) -> Self
    where
        P: Into<Option<PeerId>>,
    {
        Self {
            remote: peer.into().map(Remote::Peer).unwrap_or_default(),
            urn: Cow::Borrowed(urn),
        }
    }

    pub fn into_owned(self) -> ReferenceName<'static, R> {
        let urn = self.urn.into_owned();
        ReferenceName {
            remote: self.remote,
            urn: Cow::Owned(urn),
        }
    }

    /*
        fn namespace(&self) -> String
        where
            R: HasProtocol,
            &'a R: Into<Multihash>,
        {
            self.clone().into_owned().urn.encode_id()
        }
    */
}

impl<'a, R> From<&'a ReferenceName<'a, R>> for RefLike
where
    R: HasProtocol + ToOwned + Clone,
    &'a R: Into<Multihash>,
{
    fn from(r: &'a ReferenceName<'a, R>) -> Self {
        let namespace: String = r.urn.encode_id();
        let namespace =
            RefLike::try_from(namespace).expect("namespace should be valid ref component");
        base().join(namespace).join(&r.remote)
    }
}

pub mod error {
    use link_crypto::peer;

    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum Parse {
        #[error("expected prefix `refs/rad/remotes`")]
        WrongPrefix,
        #[error("missing path component `{0}`")]
        MissingComponent(&'static str),
        #[error(transparent)]
        Peer(#[from] peer::conversion::Error),
        #[error(transparent)]
        Urn(Box<dyn std::error::Error + Send + Sync + 'static>),
    }
}

impl<'a, R> FromStr for ReferenceName<'a, R>
where
    R: TryFrom<Multihash> + ToOwned + Clone,
    R::Error: std::error::Error + Send + Sync + 'static,
{
    type Err = error::Parse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let suffix = s
            .strip_prefix("refs/rad/remotes/")
            .ok_or(error::Parse::WrongPrefix)?;

        let mut components = suffix.split('/');

        let urn = if let Some(urn) = components.next() {
            Urn::try_from_id(urn).map_err(|e| error::Parse::Urn(e.into()))?
        } else {
            return Err(error::Parse::MissingComponent("<urn>"));
        };

        let remote = if let Some(remote) = components.next() {
            remote.parse()?
        } else {
            return Err(error::Parse::MissingComponent("(default | <peer>)"));
        };

        Ok(Self {
            remote,
            urn: Cow::Owned(urn),
        })
    }
}

impl<'a, R: 'a> fmt::Display for ReferenceName<'a, R>
where
    R: HasProtocol + ToOwned + Clone,
    &'a R: Into<Multihash>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ReferenceName { urn, remote } = self.clone().into_owned();
        write!(f, "refs/rad/remotes/{}/{}", urn.encode_id(), remote)
    }
}
