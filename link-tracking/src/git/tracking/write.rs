use std::{convert::TryFrom as _, time::Duration};

use bstr::BString;
use git_repository::{
    lock,
    objs::Blob,
    prelude::{ObjectAccessExt, ReferenceAccessExt},
    refs::{
        transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog},
        FullName,
        Target,
    },
};

use link_canonical::{Canonical as _, Cstring};
use link_crypto::PeerId;
use link_identities::git::Urn;
use radicle_git_ext::Oid;

use crate::{git::tracking::reference::ReferenceRef, peer, tracking::Write};

pub mod error {
    use thiserror::Error;

    use git_repository::{
        easy::{object, reference},
        refs::name,
    };

    #[derive(Debug, Error)]
    pub enum Track {
        #[error(transparent)]
        Object(#[from] object::write::Error),

        #[error("can't track oneself")]
        SelfReferential,

        #[error(transparent)]
        ReferenceEdit(#[from] reference::edit::Error),

        #[error(transparent)]
        ReferenceFind(#[from] reference::find::Error),

        #[error(transparent)]
        ReferenceName(#[from] name::Error),
    }

    #[derive(Debug, Error)]
    pub enum Untrack {
        #[error(transparent)]
        ReferenceEdit(#[from] reference::edit::Error),

        #[error(transparent)]
        ReferenceFind(#[from] reference::find::Error),

        #[error(transparent)]
        ReferenceName(#[from] name::Error),
    }

    #[derive(Debug, Error)]
    pub enum Update {
        #[error(transparent)]
        Object(#[from] object::write::Error),

        #[error(transparent)]
        ReferenceEdit(#[from] reference::edit::Error),

        #[error(transparent)]
        ReferenceFind(#[from] reference::find::Error),

        #[error(transparent)]
        ReferenceName(#[from] name::Error),
    }
}

impl<Repo> Write<Oid, Cstring, Cstring> for Repo
where
    Repo: ReferenceAccessExt + ObjectAccessExt + peer::LocalPeer,
{
    type Track = error::Track;
    type Untrack = error::Untrack;
    type Update = error::Update;

    fn track(
        &self,
        urn: &Urn,
        peer: Option<PeerId>,
        config: Option<Self::Config>,
    ) -> Result<bool, Self::Track> {
        let local = self.local();

        if let Some(peer) = peer {
            if peer == local {
                return Err(error::Track::SelfReferential);
            }
        }

        let reference = ReferenceRef::new(urn, peer);
        let name = FullName::try_from(reference.clone())?;
        match self.try_find_reference(name.to_partial())? {
            None => {
                let target = {
                    let blob = Blob {
                        data: config.unwrap_or_default().canonical_form().unwrap(),
                    };
                    self.write_object(blob)?
                };
                self.reference(
                    reference,
                    target,
                    PreviousValue::MustNotExist,
                    "created tracking configuration",
                )?;
                Ok(true)
            },
            Some(_) => Ok(false),
        }
    }

    fn untrack(&self, urn: &Urn, peer: PeerId) -> Result<bool, Self::Untrack> {
        let reference = ReferenceRef::new(urn, peer);
        let name = FullName::try_from(reference)?;
        match self.try_find_reference(name.to_partial())? {
            None => Ok(false),
            Some(_) => {
                let backoff = Duration::from_secs(60);
                self.edit_reference(
                    RefEdit {
                        change: Change::Delete {
                            expected: PreviousValue::MustExist,
                            log: RefLog::Only,
                        },
                        name,
                        deref: false,
                    },
                    lock::acquire::Fail::AfterDurationWithBackoff(backoff),
                    None,
                )?;
                Ok(true)
            },
        }
    }

    fn update(&self, urn: &Urn, peer: PeerId, config: Self::Config) -> Result<bool, Self::Update> {
        let reference = ReferenceRef::new(urn, peer);
        let name = FullName::try_from(reference)?;
        match self.try_find_reference(name.to_partial())? {
            None => Ok(false),
            Some(_) => {
                let new = {
                    let blob = Blob {
                        data: config.canonical_form().unwrap(),
                    };
                    self.write_object(blob)?
                };
                let backoff = Duration::from_secs(60);
                self.edit_reference(
                    RefEdit {
                        change: Change::Update {
                            log: LogChange {
                                mode: RefLog::AndReference,
                                force_create_reflog: false,
                                message: BString::from("updated tracking configuration"),
                            },
                            expected: PreviousValue::MustExist,
                            new: Target::Peeled(new.into()),
                        },
                        name,
                        deref: false,
                    },
                    lock::acquire::Fail::AfterDurationWithBackoff(backoff),
                    None,
                )?;
                Ok(true)
            },
        }
    }
}
