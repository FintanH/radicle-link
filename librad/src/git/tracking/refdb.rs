// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use link_tracking::git::{
    refdb::{self, Applied, Find, PreviousError, Scan, Update, Updated, Write},
    tracking::reference::RefName,
};

use crate::{
    git::storage::{read, ReadOnly, ReadOnlyStorage, Storage},
    git_ext as ext,
};

pub mod error {
    use thiserror::Error;

    use link_tracking::git::tracking::reference;

    use crate::{git::storage::read, git_ext as ext};

    #[derive(Debug, Error)]
    #[error("the reference was symbolic, but it is expected to be direct")]
    pub struct SymbolicRef;

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
    pub enum Txn {
        #[error("failed to initialise git transaction")]
        Acquire(#[source] git2::Error),
        #[error("failed to commit git transaction")]
        Commit(#[source] git2::Error),
        #[error("failed to delete reference `{refname}`")]
        Delete {
            refname: String,
            #[source]
            source: git2::Error,
        },
        #[error("failed while acquiring lock for `{refname}`")]
        Lock {
            refname: String,
            #[source]
            source: git2::Error,
        },
        #[error(transparent)]
        Read(#[from] read::Error),
        #[error(transparent)]
        SymbolicRef(#[from] SymbolicRef),
        #[error("failed to write reference `{refname}` with target `{target}`")]
        Write {
            refname: String,
            target: ext::Oid,
            #[source]
            source: git2::Error,
        },
    }
}

fn convert(r: git2::Reference<'_>) -> Result<refdb::Ref<'static, ext::Oid>, error::Conversion> {
    let name = r.name().ok_or(error::Conversion::Format)?;
    Ok(Ref {
        name: name.parse()?,
        target: r.target().map(ext::Oid::from).ok_or(error::SymbolicRef)?,
    })
}

type Ref<'a> = refdb::Ref<'a, ext::Oid>;

pub struct References<'a> {
    inner: read::References<'a>,
}

impl<'a> Iterator for References<'a> {
    type Item = Result<Ref<'static>, error::Iter>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|reference| {
            reference
                .map_err(error::Iter::from)
                .and_then(|r| convert(r).map_err(error::Iter::from))
        })
    }
}
impl Find for ReadOnly {
    type FindError = error::Find;

    type Oid = ext::Oid;

    fn find_reference(
        &self,
        reference: &RefName<'_, Self::Oid>,
    ) -> Result<Option<Ref>, Self::FindError> {
        let gref = self.reference(&ext::RefLike::from(reference))?;
        Ok(gref
            .map(|gref| {
                let target = gref.target().map(ext::Oid::from).ok_or(error::SymbolicRef);
                target.map(|target| Ref {
                    name: reference.clone().into_owned(),
                    target,
                })
            })
            .transpose()?)
    }
}

impl<'a> Scan for &'a ReadOnly {
    type ReferencesError = read::Error;
    type IterError = error::Iter;

    type Oid = ext::Oid;
    type References = References<'a>;

    fn references(
        self,
        spec: &ext::RefspecPattern,
    ) -> Result<Self::References, Self::ReferencesError> {
        let references = ReadOnlyStorage::references(self, spec)?;
        Ok(References { inner: references })
    }
}

impl Find for Storage {
    type FindError = error::Find;

    type Oid = ext::Oid;

    fn find_reference(
        &self,
        reference: &RefName<'_, Self::Oid>,
    ) -> Result<Option<Ref>, Self::FindError> {
        self.read_only().find_reference(reference)
    }
}

impl<'a> Scan for &'a Storage {
    type ReferencesError = read::Error;
    type IterError = error::Iter;

    type Oid = ext::Oid;
    type References = References<'a>;

    fn references(
        self,
        spec: &ext::RefspecPattern,
    ) -> Result<Self::References, Self::ReferencesError> {
        Scan::references(self.read_only(), spec)
    }
}

impl Write for Storage {
    type TxnError = error::Txn;

    type Oid = ext::Oid;

    fn update<'a, I>(&self, updates: I) -> Result<Applied<'a, Self::Oid>, Self::TxnError>
    where
        I: IntoIterator<Item = Update<'a, Self::Oid>>,
    {
        let raw = self.as_raw();
        let mut txn = raw.transaction().map_err(error::Txn::Acquire)?;
        let mut applied = Applied::default();
        let mut reject_or_update =
            |previous: Option<PreviousError<Self::Oid>>, update: Updated<'a, Self::Oid>| {
                match previous {
                    None => applied.updates.push(update),
                    Some(rejection) => applied.rejections.push(rejection),
                }
            };

        for update in updates {
            match update {
                Update::Write {
                    name,
                    target,
                    previous,
                } => {
                    let refname = name.to_string();
                    let message = &format!("writing reference with target `{}`", target);
                    txn.lock_ref(&refname).map_err(|err| error::Txn::Lock {
                        refname: refname.clone(),
                        source: err,
                    })?;
                    let set = || -> Result<(), Self::TxnError> {
                        txn.set_target(&refname, target.into(), None, message)
                            .map_err(|err| error::Txn::Write {
                                refname,
                                target,
                                source: err,
                            })
                    };
                    match self.reference(&name)? {
                        Some(r) => reject_or_update(
                            previous.guard(r.target().map(ext::Oid::from).as_ref(), set)?,
                            Updated::Written { name, target },
                        ),
                        None => reject_or_update(
                            previous.guard(None, set)?,
                            Updated::Written { name, target },
                        ),
                    }
                },
                Update::Delete { name, previous } => {
                    let refname = name.to_string();
                    txn.lock_ref(&refname).map_err(|err| error::Txn::Lock {
                        refname: refname.clone(),
                        source: err,
                    })?;
                    let delete = || -> Result<(), Self::TxnError> {
                        txn.remove(&refname).map_err(|err| error::Txn::Delete {
                            refname,
                            source: err,
                        })
                    };
                    match self.reference(&name)? {
                        Some(r) => reject_or_update(
                            previous.guard(r.target().map(ext::Oid::from).as_ref(), delete)?,
                            Updated::Deleted {
                                name,
                                previous: Some(
                                    r.target()
                                        .map(Ok)
                                        .unwrap_or(Err(error::SymbolicRef))?
                                        .into(),
                                ),
                            },
                        ),
                        None => reject_or_update(
                            previous.guard(None, delete)?,
                            Updated::Deleted {
                                name,
                                previous: None,
                            },
                        ),
                    }
                },
            }
        }
        txn.commit().map_err(error::Txn::Commit)?;
        Ok(applied)
    }
}
