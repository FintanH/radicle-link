use std::{
    convert::TryFrom,
    fmt,
    path::{Component, Path},
};

use git_repository::refs::{name, FullName};
use link_crypto::PeerId;
use link_identities::git::Urn;

#[derive(Clone, Copy, Debug)]
pub enum Remote {
    Default,
    Peer(PeerId),
}

impl fmt::Display for Remote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Remote::Default => write!(f, "default"),
            Remote::Peer(peer) => write!(f, "{}", peer),
        }
    }
}

impl Default for Remote {
    fn default() -> Self {
        Self::Default
    }
}

impl<'a> TryFrom<&'a Remote> for FullName {
    type Error = name::Error;

    fn try_from(remote: &'a Remote) -> Result<Self, Self::Error> {
        let remote = remote.to_string();
        Self::try_from(remote.as_str())
    }
}

pub struct Reference {
    pub remote: Remote,
    pub urn: Urn,
}

impl Reference {
    pub fn new<P>(urn: Urn, peer: P) -> Self
    where
        P: Into<Option<PeerId>>,
    {
        Self {
            remote: peer.into().map(Remote::Peer).unwrap_or_default(),
            urn,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReferenceRef<'a> {
    pub remote: Remote,
    pub namespace: &'a Urn,
}

impl<'a> ReferenceRef<'a> {
    pub fn new<P>(urn: &'a Urn, peer: P) -> Self
    where
        P: Into<Option<PeerId>>,
    {
        Self {
            remote: peer.into().map(Remote::Peer).unwrap_or_default(),
            namespace: urn,
        }
    }

    pub fn into_owned(&self) -> Reference {
        Reference {
            remote: self.remote.clone(),
            urn: self.namespace.clone(),
        }
    }
}

impl<'a> TryFrom<ReferenceRef<'a>> for FullName {
    type Error = name::Error;

    fn try_from(refl: ReferenceRef<'a>) -> Result<Self, Self::Error> {
        Self::try_from(
            format!(
                "refs/rad/remotes/{}/{}",
                refl.namespace.encode_id(),
                refl.remote
            )
            .as_str(),
        )
    }
}

pub mod error {
    use thiserror::Error;

    use link_crypto::peer;
    use link_identities::urn::error::DecodeId;
    use radicle_git_ext::FromMultihashError;

    #[derive(Debug, Error)]
    pub enum Path {
        #[error("the remote component is missing, expected `default` or valid peer id")]
        MissingRemote,
        #[error("the namespace component is missing")]
        MissingNamespace,
        #[error(transparent)]
        Peer(#[from] peer::conversion::Error),
        #[error(transparent)]
        Urn(#[from] DecodeId<FromMultihashError>),
        #[error("could not convert path component to utf-8")]
        Malformed,
    }
}

impl<'a> TryFrom<&'a Path> for Reference {
    type Error = error::Path;

    fn try_from(path: &'a Path) -> Result<Self, Self::Error> {
        let mut components = path.components();

        let remote = if let Some(Component::Normal(remote)) = components.next_back() {
            match remote.to_str().ok_or_else(|| error::Path::Malformed)? {
                "default" => Remote::Default,
                peer => peer.parse::<PeerId>().map(Remote::Peer)?,
            }
        } else {
            return Err(error::Path::MissingRemote);
        };

        let urn = if let Some(Component::Normal(namespace)) = components.next_back() {
            Urn::try_from_id(namespace.to_str().unwrap())?
        } else {
            return Err(error::Path::MissingNamespace);
        };
        Ok(Reference { urn, remote })
    }
}
