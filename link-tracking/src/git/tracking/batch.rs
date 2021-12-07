// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::collections::HashMap;

use link_crypto::PeerId;
use link_identities::urn::Urn;
use radicle_git_ext::Oid;

use super::{config::Config, error, odb, refdb, Ref, RefName};

/// A tracking action that performs a write during a [`batch`] operation.
pub enum Action<'a, Oid> {
    Track {
        urn: &'a Urn<Oid>,
        peer: Option<PeerId>,
        config: Config,
    },
    Untrack {
        urn: &'a Urn<Oid>,
        peer: PeerId,
    },
    Update {
        urn: &'a Urn<Oid>,
        peer: Option<PeerId>,
        config: Config,
    },
}

/// The applied updates for the given set of [`Action`]s in a [`batch`]
/// operation.
pub struct Applied {
    pub updates: Vec<Updated>,
}

pub enum Updated {
    /// The `Ref` was either created during an [Action::Track] or updated
    /// during a [Action::Update].
    Tracked { reference: Ref },
    /// The `RefName` was removed during an [Action::Untrack].
    Untracked { name: RefName<'static, Oid> },
}

impl<'a> From<refdb::Updated<'a, Oid>> for Updated {
    fn from(updated: refdb::Updated<Oid>) -> Updated {
        match updated {
            refdb::Updated::Written { name, target } => Self::Tracked {
                reference: Ref {
                    name: name.clone().into_owned(),
                    target,
                },
            },
            refdb::Updated::Deleted { name } => Self::Untracked {
                name: name.clone().into_owned(),
            },
        }
    }
}

/// Perform a transactional update of the provided `actions`.
///
/// # Note
///
/// The transactional nature of the operation depends on the implementation of
/// [`refdb::Write::update`].
///
/// Any [`Config`]s that require writing to the `Odb` are not part of the
/// transaction and happen before the references are updated.
pub fn batch<'a, Db, I>(db: &'a Db, actions: I) -> Result<Applied, error::Batch>
where
    Db: odb::Write<Oid = Oid> + refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
    I: IntoIterator<Item = Action<'a, Oid>> + 'a,
{
    let updates = into_updates(db, actions).collect::<Result<Vec<_>, _>>()?;
    let applied = db
        .update(updates)
        .map_err(|err| error::Batch::Txn { source: err.into() })?;
    Ok(Applied {
        updates: applied.updates.into_iter().map(Updated::from).collect(),
    })
}

fn into_updates<'a, Db, I>(
    db: &'a Db,
    actions: I,
) -> impl Iterator<Item = Result<refdb::Update<'a, Oid>, error::Batch>> + 'a
where
    Db: odb::Write<Oid = Oid> + refdb::Read<Oid = Oid>,
    I: IntoIterator<Item = Action<'a, Oid>> + 'a,
{
    let mut seen: HashMap<Config, Oid> = HashMap::new();
    actions.into_iter().filter_map(move |action| match action {
        Action::Track { urn, peer, config } => {
            let name = RefName::borrowed(urn, peer);
            on_missing(db, &name.clone(), || {
                target(db, &mut seen, &name, config)
                    .map(|target| refdb::Update::Write { name, target })
            })
            .transpose()
        },
        Action::Untrack { urn, peer } => {
            let name = RefName::borrowed(urn, peer);
            Some(Ok(refdb::Update::Delete { name }))
        },
        Action::Update { urn, peer, config } => {
            let name = RefName::borrowed(urn, peer);
            on_existing(db, &name.clone(), || {
                target(db, &mut seen, &name, config)
                    .map(|target| refdb::Update::Write { name, target })
            })
            .transpose()
        },
    })
}

fn target<'a, Db>(
    db: &Db,
    cache: &mut HashMap<Config, Oid>,
    name: &RefName<'a, Oid>,
    config: Config,
) -> Result<Oid, error::Batch>
where
    Db: odb::Write<Oid = Oid>,
{
    match cache.get(&config) {
        None => {
            let target = db
                .write_config(&config)
                .map_err(|err| error::Batch::WriteObj {
                    reference: name.clone().into_owned(),
                    source: err.into(),
                })?;
            cache.insert(config, target);
            Ok(target)
        },
        Some(target) => Ok(*target),
    }
}

fn on_existing<'a, Db>(
    db: &Db,
    name: &RefName<'a, Oid>,
    callback: impl FnOnce() -> Result<refdb::Update<'a, Oid>, error::Batch>,
) -> Result<Option<refdb::Update<'a, Oid>>, error::Batch>
where
    Db: refdb::Read<Oid = Oid>,
{
    db.find_reference(name)
        .map_err(|err| error::Batch::FindRef {
            reference: name.clone().into_owned(),
            source: err.into(),
        })?
        .map(|_| callback())
        .transpose()
}

fn on_missing<'a, Db>(
    db: &Db,
    name: &RefName<'a, Oid>,
    callback: impl FnOnce() -> Result<refdb::Update<'a, Oid>, error::Batch>,
) -> Result<Option<refdb::Update<'a, Oid>>, error::Batch>
where
    Db: refdb::Read<Oid = Oid>,
{
    match db
        .find_reference(name)
        .map_err(|err| error::Batch::FindRef {
            reference: name.clone().into_owned(),
            source: err.into(),
        })? {
        None => callback().map(Some),
        Some(_) => Ok(None),
    }
}
