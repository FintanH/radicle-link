use std::{convert::TryFrom, path::Path};

use git_repository::{
    easy,
    prelude::{ObjectAccessExt, ReferenceAccessExt},
    refs::FullName,
};

use link_canonical::Cstring;
use link_crypto::PeerId;
use link_identities::git::Urn;
use radicle_git_ext::Oid;

use crate::{
    git::{
        config::Config,
        tracking::reference::{self, Reference, ReferenceRef, Remote},
    },
    tracking::{Read, Tracked},
};

pub mod error {
    use git_repository::{
        easy::{object, reference},
        refs::name,
    };
    use thiserror::Error;

    use crate::git::{config, tracking};

    #[derive(Debug, Error)]
    pub enum Tracked {
        #[error(transparent)]
        Config(#[from] config::error::Git),

        #[error(transparent)]
        Iter(#[from] reference::iter::Error),

        #[error(transparent)]
        IterInit(#[from] reference::iter::init::Error),

        #[error(transparent)]
        ObjectFind(#[from] object::find::existing::Error),

        #[error(transparent)]
        Reference(Box<dyn std::error::Error + Send + Sync + 'static>),

        #[error(transparent)]
        TrackingReference(#[from] tracking::reference::error::Path),
    }

    #[derive(Debug, Error)]
    pub enum Get {
        #[error(transparent)]
        Config(#[from] config::error::Git),

        #[error(transparent)]
        ObjectFind(#[from] object::find::existing::Error),

        #[error(transparent)]
        ReferenceFind(#[from] reference::find::Error),

        #[error(transparent)]
        ReferenceName(#[from] name::Error),

        #[error(transparent)]
        TrackingReference(#[from] tracking::reference::error::Path),
    }
}

impl<Repo> Read<Oid, Cstring, Cstring> for Repo
where
    Repo: ReferenceAccessExt + ObjectAccessExt,
{
    type Tracked = error::Tracked;
    type Get = error::Get;

    fn tracked(
        &self,
        filter_by: Option<&Urn>,
    ) -> Result<Vec<Tracked<Oid, Self::Config>>, Self::Tracked> {
        let prefix = Path::new("refs/rad/remotes");

        match filter_by {
            Some(urn) => prefix.join(urn.encode_id()),
            None => prefix.to_path_buf(),
        };
        let mut references = vec![];

        for reference in self.references()?.prefixed(&prefix)? {
            let reference = reference.map_err(error::Tracked::Reference)?;
            let id = reference.id();
            let obj = self.find_object(id)?;
            let config = Config::try_from(&obj)?;
            references.push(try_from_reference(reference, config)?)
        }

        Ok(references)
    }

    fn get(
        &self,
        urn: &Urn,
        peer: Option<PeerId>,
    ) -> Result<Option<Tracked<Oid, Self::Config>>, Self::Get> {
        let reference = ReferenceRef::new(urn, peer);
        match self.try_find_reference(FullName::try_from(reference)?.to_partial())? {
            None => Ok(None),
            Some(reference) => {
                let id = reference.id();
                let obj = self.find_object(id)?;
                let config = Config::try_from(&obj)?;
                try_from_reference(reference, config)
                    .map(Some)
                    .map_err(error::Get::from)
            },
        }
    }

    fn is_tracked(&self, urn: &Urn, peer: Option<PeerId>) -> Result<bool, Self::Get> {
        let reference = ReferenceRef::new(urn, peer);
        self.try_find_reference(FullName::try_from(reference)?.to_partial())
            .map(|r| r.is_some())
            .map_err(error::Get::from)
    }
}

fn try_from_reference<A>(
    reference: easy::Reference<'_, A>,
    config: Config,
) -> Result<Tracked<Oid, Config>, reference::error::Path> {
    let name = reference.name().to_path();
    let reference = Reference::try_from(name.as_ref())?;

    Ok(match reference.remote {
        Remote::Default => Tracked::Default {
            urn: reference.urn,
            config,
        },
        Remote::Peer(peer) => Tracked::Peer {
            urn: reference.urn,
            peer,
            config,
        },
    })
}
