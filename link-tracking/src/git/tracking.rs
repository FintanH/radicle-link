// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{collections::BTreeMap, convert::TryFrom};

use tracing::warn;

use link_crypto::PeerId;
use link_identities::urn::Urn;
use radicle_git_ext::{Oid, RefLike};

use crate::tracking::Tracked;

use super::{
    config::{self, Config},
    odb,
    refdb,
};

pub mod batch;
pub use batch::{batch, Action, Applied};
pub mod error;
pub mod reference;
pub use reference::{RefName, Remote};

pub type Ref = refdb::Ref<'static, Oid>;

/// Track the `urn` for the given `peer`, storing the provided `config` at
/// `refs/rad/remotes/<urn>/(<peer> | default)`.
///
/// If `peer` is `None`, the `default` entry is created.
///
/// Use `Default` instance of `Config` to allow all references to be fetched for
/// the given peer. Otherwise see [`Config`] for details on restricting
/// references.
///
/// The [`Ref`] that was created is returned if the tracking entry was newly
/// created, otherwise if the entry already existed, then `None` is returned.
pub fn track<Db>(
    db: &Db,
    urn: &Urn<Oid>,
    peer: Option<PeerId>,
    config: Config,
) -> Result<Option<Ref>, error::Track>
where
    Db: odb::Read<Oid = Oid>
        + odb::Write<Oid = Oid>
        + refdb::Read<Oid = Oid>
        + refdb::Write<Oid = Oid>,
{
    use error::Track;

    let reference = RefName::borrowed(urn, peer);
    match load_config(db, &reference).map_err(|err| Track::FindObj {
        reference: reference.clone().into_owned(),
        source: err.into(),
    })? {
        None => {
            let target = db.write_config(&config).map_err(|err| Track::WriteObj {
                reference: reference.clone().into_owned(),
                source: err.into(),
            })?;
            let update = vec![refdb::Update::Write {
                name: reference.clone(),
                target,
            }];
            db.update(update)
                .map(|refdb::Applied { updates }| {
                    updates.first().and_then(|updated| match updated {
                        refdb::Updated::Written { name, target } => Some(Ref {
                            name: name.clone().into_owned(),
                            target: *target,
                        }),
                        refdb::Updated::Deleted { .. } => panic!("write update was expected"),
                    })
                })
                .map_err(|err| Track::Create {
                    reference: reference.into_owned(),
                    source: err.into(),
                })
        },
        Some(_) => Ok(None),
    }
}

/// Untrack the `urn` for the given `peer`, removing the reference
/// `refs/rad/remotes/<urn>/<peer>`.
///
/// If the tracking did not exist `None` is returned. Otherwise, if the untrack
/// was successful then the [`Tracked`] entry is returned.
pub fn untrack<Db>(
    db: &Db,
    urn: &Urn<Oid>,
    peer: PeerId,
) -> Result<Option<Tracked<Oid, Config>>, error::Untrack>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
{
    use error::Untrack;

    let reference = RefName::borrowed(urn, peer);
    match load_config(db, &reference).map_err(|err| Untrack::FindObj {
        reference: reference.clone().into_owned(),
        source: err.into(),
    })? {
        None => Ok(None),
        Some(config) => {
            let updates = vec![refdb::Update::Delete {
                name: reference.clone(),
            }];
            db.update(updates)
                .map_err(|err| Untrack::Delete {
                    reference: reference.clone().into_owned(),
                    source: err.into(),
                })
                .map(|_| Some(from_reference(&reference, config)))
        },
    }
}

/// Untrack all peers under `urn`, removing all references
/// `refs/rad/remotes/<urn>/*`.
///
/// The [`RefName`] of each deleted reference is returned.
pub fn untrack_all<Db>(
    db: &Db,
    urn: &Urn<Oid>,
) -> Result<Vec<RefName<'static, Oid>>, error::UntrackAll>
where
    Db: refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
{
    use error::UntrackAll;

    let prefix = reflike!("refs/rad/remotes");
    let namespace =
        RefLike::try_from(urn.encode_id()).expect("namespace should be valid ref component");
    let spec = prefix
        .join(namespace)
        .with_pattern_suffix(refspec_pattern!("*"));
    let updates = {
        let refs = db.references(&spec).map_err(|err| UntrackAll::References {
            spec: spec.clone(),
            source: err.into(),
        })?;
        refs.into_iter()
            .map(|r| r.map(|r| refdb::Update::Delete { name: r.name }))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| UntrackAll::Iter {
                spec: spec.clone(),
                source: err.into(),
            })?
    };
    db.update(updates)
        .map(|refdb::Applied { updates }| {
            updates
                .into_iter()
                .map(|updated| match updated {
                    refdb::Updated::Written { .. } => panic!("no write was expected"),
                    refdb::Updated::Deleted { name } => name.clone().into_owned(),
                })
                .collect()
        })
        .map_err(|err| UntrackAll::Delete {
            spec,
            source: err.into(),
        })
}

/// Update the tracking entry for the given `urn` and `peer`, storing the
/// provided `config` at `refs/rad/remotes/<urn>/(<peer> | default)`.
///
/// If `peer` is `None`, the `default` entry is created.
///
/// # Note
///
/// This operation will return `false` if the tracking entry
/// did not exist.
pub fn update<Db>(
    db: &Db,
    urn: &Urn<Oid>,
    peer: Option<PeerId>,
    config: Config,
) -> Result<Option<Ref>, error::Update>
where
    Db: odb::Write<Oid = Oid> + refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
{
    use error::Update;

    let name = RefName::borrowed(urn, peer);
    match db.find_reference(&name).map_err(|err| Update::FindRef {
        reference: name.clone().into_owned(),
        source: err.into(),
    })? {
        None => Ok(None),
        Some(_) => {
            let target = db.write_config(&config).map_err(|err| Update::WriteObj {
                reference: name.clone().into_owned(),
                source: err.into(),
            })?;
            let updates = vec![refdb::Update::Write {
                name: name.clone(),
                target,
            }];
            db.update(updates).map_err(|err| Update::WriteRef {
                object: target,
                reference: name.clone().into_owned(),
                source: err.into(),
            })?;
            Ok(Some(refdb::Ref {
                name: name.clone().into_owned(),
                target,
            }))
        },
    }
}

pub struct TrackedEntries<'a> {
    inner: Box<dyn Iterator<Item = Result<Tracked<Oid, Config>, error::Tracked>> + 'a>,
}

impl<'a> Iterator for TrackedEntries<'a> {
    type Item = Result<Tracked<Oid, Config>, error::Tracked>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Return all tracked entries, optionally filtering by an [`Urn`].
pub fn tracked<'a, Db>(
    db: &'a Db,
    filter_by: Option<&Urn<Oid>>,
) -> Result<TrackedEntries<'a>, error::Tracked>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    let prefix = reflike!("refs/rad/remotes}");
    let spec = match filter_by {
        Some(urn) => {
            let namespace = RefLike::try_from(urn.encode_id())
                .expect("namespace should be valid ref component");
            prefix
                .join(namespace)
                .with_pattern_suffix(refspec_pattern!("*"))
        },
        None => prefix.with_pattern_suffix(refspec_pattern!("*")),
    };
    let seen: BTreeMap<Oid, Config> = BTreeMap::new();
    let resolve = {
        let spec = spec.clone();
        move |reference: Result<refdb::Ref<Oid>, Db::IterError>| {
            let reference = reference.map_err(|err| error::Tracked::Iter {
                spec: spec.clone(),
                source: err.into(),
            })?;

            // We may have seen this config already
            if let Some(config) = seen.get(&reference.target) {
                return Ok(Some(from_reference(&reference.name, config.clone())));
            }

            // Otherwise we attempt to fetch it from the backend
            match db
                .find_config(&reference.target)
                .map_err(|err| error::Tracked::FindObj {
                    reference: reference.name.clone().into_owned(),
                    target: reference.target,
                    source: err.into(),
                })? {
                None => {
                    warn!(name=?reference.name, oid=?reference.target, "missing blob");
                    Ok(None)
                },
                Some(config) => Ok(Some(from_reference(&reference.name, config))),
            }
        }
    };

    Ok(TrackedEntries {
        inner: Box::new(
            db.references(&spec)
                .map_err(|err| error::Tracked::References {
                    spec: spec.clone(),
                    source: err.into(),
                })?
                .into_iter()
                .filter_map(move |r| resolve(r).transpose()),
        ),
    })
}

pub struct TrackedPeers<'a> {
    inner: Box<dyn Iterator<Item = Result<PeerId, error::TrackedPeers>> + 'a>,
}

impl<'a> Iterator for TrackedPeers<'a> {
    type Item = Result<PeerId, error::TrackedPeers>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Return all tracked peers, optionally filtering by an [`Urn`].
pub fn tracked_peers<'a, Db>(
    db: &'a Db,
    filter_by: Option<&Urn<Oid>>,
) -> Result<TrackedPeers<'a>, error::TrackedPeers>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    let prefix = reflike!("refs/rad/remotes");
    let spec = match filter_by {
        Some(urn) => {
            let namespace = RefLike::try_from(urn.encode_id())
                .expect("namespace should be valid ref component");
            prefix
                .join(namespace)
                .with_pattern_suffix(refspec_pattern!("*"))
        },
        None => prefix.with_pattern_suffix(refspec_pattern!("*")),
    };

    let resolve = {
        let spec = spec.clone();
        move |reference: Result<refdb::Ref<Oid>, Db::IterError>| -> Result<Option<PeerId>, error::TrackedPeers> {
            let reference = reference.map_err(|err| error::TrackedPeers::Iter {
                spec: spec.clone(),
                source: err.into(),
            })?;

            Ok(reference.name.remote.into())
        }
    };

    Ok(TrackedPeers {
        inner: Box::new(
            db.references(&spec)
                .map_err(|err| error::TrackedPeers::References {
                    spec: spec.clone(),
                    source: err.into(),
                })?
                .into_iter()
                .filter_map(move |r| resolve(r).transpose()),
        ),
    })
}

/// Return a tracking entry for a given `urn` and `peer`.
///
/// If `refs/rad/remotes/<urn>/(<peer> | default)` does not exist, then `None`
/// is returned.
pub fn get<Db>(
    db: &Db,
    urn: &'_ Urn<Oid>,
    peer: Option<PeerId>,
) -> Result<Option<Tracked<Oid, Config>>, error::Get>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    use error::Get;

    let name = RefName::borrowed(urn, peer);
    match db.find_reference(&name).map_err(|err| Get::FindRef {
        reference: name.clone().into_owned(),
        source: err.into(),
    })? {
        None => Ok(None),
        Some(reference) => match db
            .find_config(&reference.target)
            .map_err(|err| Get::FindObj {
                reference: reference.name.into_owned(),
                target: reference.target,
                source: err.into(),
            })? {
            None => Ok(None),
            Some(config) => Ok(Some(from_reference(&name, config))),
        },
    }
}

/// Check a tracking entry for a given `urn` and `peer` exists.
pub fn is_tracked<Db>(
    db: &Db,
    urn: &Urn<Oid>,
    peer: Option<PeerId>,
) -> Result<bool, error::IsTracked>
where
    Db: refdb::Read<Oid = Oid>,
{
    use error::IsTracked;

    let name = RefName::borrowed(urn, peer);
    match db.find_reference(&name).map_err(|err| IsTracked::FindRef {
        reference: name.into_owned(),
        source: err.into(),
    })? {
        None => Ok(false),
        Some(_) => Ok(true),
    }
}

/// Check that the only tracking entry for the given `urn` is the default entry.
/// This will return false if there are either:
///   * No tracking entries for the `urn`
///   * There is at least one tracked peer for the `urn`
pub fn default_only<Db>(db: &Db, urn: &Urn<Oid>) -> Result<bool, error::DefaultOnly>
where
    Db: refdb::Read<Oid = Oid>,
{
    use error::DefaultOnly;

    let spec = {
        let namespace =
            RefLike::try_from(urn.encode_id()).expect("namespace should be valid ref component");
        reference::base()
            .join(namespace)
            .with_pattern_suffix(refspec_pattern!("*"))
    };
    let mut seen_default = false;
    for reference in db
        .references(&spec)
        .map_err(|err| DefaultOnly::References {
            spec: spec.clone(),
            source: err.into(),
        })?
    {
        match reference
            .map_err(|err| DefaultOnly::Iter {
                spec: spec.clone(),
                source: err.into(),
            })?
            .name
            .remote
        {
            Remote::Default => {
                seen_default = true;
            },
            Remote::Peer(_) => return Ok(false),
        }
    }

    Ok(seen_default)
}

fn from_reference(reference: &RefName<'_, Oid>, config: Config) -> Tracked<Oid, Config> {
    match reference.remote {
        Remote::Default => Tracked::Default {
            urn: reference.urn.clone().into_owned(),
            config,
        },
        Remote::Peer(peer) => Tracked::Peer {
            urn: reference.urn.clone().into_owned(),
            peer,
            config,
        },
    }
}

fn load_config<Db>(db: &Db, reference: &RefName<'_, Oid>) -> Result<Option<Config>, error::Config>
where
    Db: refdb::Read<Oid = Oid> + odb::Read<Oid = Oid>,
{
    use error::Config;

    match db
        .find_reference(reference)
        .map_err(|err| Config::FindRef {
            reference: reference.clone().into_owned(),
            source: err.into(),
        })? {
        None => Ok(None),
        Some(r) => Ok(db.find_config(&r.target).map_err(|err| Config::FindObj {
            reference: r.name.into_owned(),
            target: r.target,
            source: err.into(),
        })?),
    }
}
