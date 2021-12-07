// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use link_tracking::git::{
    refdb::{self, Read, Write},
    tracking::reference::ReferenceRef,
};

use crate::{
    git::storage::{read, ReadOnly, ReadOnlyStorage, Storage},
    git_ext as ext,
};

pub mod error {
    use thiserror::Error;

    use link_tracking::git::tracking::reference;

    use crate::git::storage::read;

    #[derive(Debug, Error)]
    #[error("the reference was symbolic, but it is expected to be direct")]
    pub struct SymbolicRef;

    #[derive(Debug, Error)]
    pub enum Create {
        #[error(transparent)]
        Conversion(#[from] Conversion),
        #[error(transparent)]
        Storage(#[from] git2::Error),
    }

    #[derive(Debug, Error)]
    pub enum Conversion {
        #[error("failed to parse reference name format")]
        Format,
        #[error(transparent)]
        SymbolicRef(#[from] SymbolicRef),
        #[error(transparent)]
        Parse(#[from] reference::error::Parse),
    }

    #[derive(Debug, Error)]
    pub enum Delete {
        #[error(transparent)]
        Conversion(#[from] Conversion),
        #[error(transparent)]
        Storage(#[from] read::Error),
    }

    #[derive(Debug, Error)]
    pub enum Find {
        #[error(transparent)]
        Storage(#[from] read::Error),
        #[error(transparent)]
        SymbolicRef(#[from] SymbolicRef),
    }

    #[derive(Debug, Error)]
    pub enum Iter {
        #[error(transparent)]
        Storage(#[from] read::Error),
        #[error(transparent)]
        Conversion(#[from] Conversion),
    }

    #[derive(Debug, Error)]
    pub enum Write {
        #[error(transparent)]
        Conversion(#[from] Conversion),
        #[error(transparent)]
        Storage(#[from] read::Error),
    }
}

fn convert(r: git2::Reference<'_>) -> Result<Ref, error::Conversion> {
    let name = r.name().ok_or(error::Conversion::Format)?;
    Ok(Ref {
        name: name.parse()?,
        target: r.target().map(ext::Oid::from).ok_or(error::SymbolicRef)?,
    })
}

type Ref = refdb::Ref<ext::Oid>;

impl Read for ReadOnly {
    type FindError = error::Find;
    type ReferencesError = read::Error;
    type IterError = error::Iter;

    type Oid = ext::Oid;

    fn find_reference(
        &self,
        reference: &ReferenceRef<'_, Self::Oid>,
    ) -> Result<Option<Ref>, Self::FindError> {
        let gref = self.reference(&ext::RefLike::from(reference))?;
        Ok(gref
            .map(|gref| {
                let target = gref.target().map(ext::Oid::from).ok_or(error::SymbolicRef);
                target.map(|target| Ref {
                    name: reference.into_owned(),
                    target,
                })
            })
            .transpose()?)
    }

    fn references(
        &self,
        spec: &ext::RefspecPattern,
    ) -> Result<Vec<Result<Ref, Self::IterError>>, Self::ReferencesError> {
        let references = ReadOnlyStorage::references(self, spec)?;
        Ok(references
            .map(|reference| {
                reference
                    .map_err(error::Iter::from)
                    .and_then(|r| convert(r).map_err(error::Iter::from))
            })
            .collect())
    }
}

impl Read for Storage {
    type FindError = error::Find;
    type ReferencesError = read::Error;
    type IterError = error::Iter;

    type Oid = ext::Oid;

    fn find_reference(
        &self,
        reference: &ReferenceRef<'_, Self::Oid>,
    ) -> Result<Option<Ref>, Self::FindError> {
        self.read_only().find_reference(reference)
    }

    fn references(
        &self,
        spec: &ext::RefspecPattern,
    ) -> Result<Vec<Result<Ref, Self::IterError>>, Self::ReferencesError> {
        Read::references(self.read_only(), spec)
    }
}

impl Write for Storage {
    type CreateError = error::Create;
    type WriteError = error::Write;
    type DeleteError = error::Delete;

    type Oid = ext::Oid;

    fn create(
        &self,
        name: &ReferenceRef<'_, Self::Oid>,
        target: Self::Oid,
    ) -> Result<Ref, Self::CreateError> {
        let raw = self.as_raw();

        let r = {
            let name = ext::RefLike::from(name);
            println!("wut {}", name);
            let message = &format!("created reference with target `{}`", target);
            raw.reference(name.as_str(), target.into(), false, message)?
        };
        Ok(convert(r)?)
    }

    // TODO(finto): Consider returning Option<Ref>
    fn write_target(
        &self,
        reference: &ReferenceRef<'_, Self::Oid>,
        target: Self::Oid,
    ) -> Result<(), Self::WriteError> {
        match self.reference(reference)? {
            None => Ok(()),
            Some(mut r) => {
                let message = &format!("update reference with target `{}`", target);
                r.set_target(target.into(), message)
                    .map_err(read::Error::from)?;
                Ok(())
            },
        }
    }

    fn delete_reference(
        &self,
        reference: &ReferenceRef<'_, Self::Oid>,
    ) -> Result<bool, Self::DeleteError> {
        match self.reference(reference)? {
            None => Ok(false),
            Some(mut r) => {
                r.delete().map_err(read::Error::from)?;
                Ok(true)
            },
        }
    }
}
