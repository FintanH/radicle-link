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

use super::{config, odb, refdb};

pub mod error;
pub mod reference;
pub use reference::{ReferenceName, Remote};

pub fn track<'a, Db>(
    db: &Db,
    urn: &'a Urn<Oid>,
    peer: Option<PeerId>,
    config: config::Config,
) -> Result<bool, error::Track>
where
    Db: odb::Read<Oid = Oid>
        + odb::Write<Oid = Oid>
        + refdb::Read<Oid = Oid>
        + refdb::Write<Oid = Oid>,
{
    use error::Track;

    let reference = ReferenceName::borrowed(urn, peer);
    match load_config(db, &reference).map_err(|err| Track::FindObj {
        reference: reference.clone().into_owned(),
        source: err.into(),
    })? {
        None => {
            let target = db.write_config(&config).map_err(|err| Track::WriteObj {
                reference: reference.clone().into_owned(),
                source: err.into(),
            })?;
            db.create(&reference, target)
                .map(|_| true)
                .map_err(|err| Track::Create {
                    reference: reference.into_owned(),
                    source: err.into(),
                })
        },
        Some(_) => Ok(false),
    }
}

// TODO(finto): if peer is None we untrack ALL of the URN
pub fn untrack<'a, Db>(
    db: &Db,
    urn: &'a Urn<Oid>,
    peer: Option<PeerId>,
) -> Result<bool, error::Untrack<'a>>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
{
    use error::Untrack;

    let reference = ReferenceName::borrowed(urn, peer);
    let mk_ref = || reference.to_owned();
    match load_config(db, &reference).map_err(|err| Untrack::FindObj {
        reference: mk_ref(),
        source: err.into(),
    })? {
        None => Ok(false),
        Some(_) => db
            .delete_reference(&reference)
            .map_err(|err| Untrack::Delete {
                reference: mk_ref(),
                source: err.into(),
            }),
    }
}

pub fn update<'a, Db>(
    db: &Db,
    urn: &'a Urn<Oid>,
    peer: Option<PeerId>,
    config: config::Config,
) -> Result<bool, error::Update<'a>>
where
    Db: odb::Write<Oid = Oid> + refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
{
    use error::Update;

    let name = ReferenceName::borrowed(urn, peer);
    let mk_ref = || name.to_owned();
    match db.find_reference(&name).map_err(|err| Update::FindRef {
        reference: mk_ref(),
        source: err.into(),
    })? {
        None => Ok(false),
        Some(reference) => {
            let oid = db.write_config(&config).map_err(|err| Update::WriteObj {
                reference: mk_ref(),
                source: err.into(),
            })?;
            db.write_target(&reference.name, oid)
                .map_err(|err| Update::WriteRef {
                    object: oid,
                    reference: mk_ref(),
                    source: err.into(),
                })?;
            Ok(true)
        },
    }
}

pub fn tracked<'a, Db>(
    db: &'a Db,
    filter_by: Option<&'_ Urn<Oid>>,
) -> Result<
    impl Iterator<Item = Result<Tracked<Oid, config::Config>, error::Tracked>> + 'a,
    error::Tracked,
>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    use error::Tracked;

    let prefix = reflike!("refs/rad/remotes");
    let pattern = match filter_by {
        Some(urn) => {
            let namespace = RefLike::try_from(urn.encode_id())
                .expect("namespace should be valid ref component");
            prefix
                .join(namespace)
                .with_pattern_suffix(refspec_pattern!("*"))
        },
        None => prefix.with_pattern_suffix(refspec_pattern!("*")),
    };
    let seen: BTreeMap<Oid, config::Config> = BTreeMap::new();
    let resolve = {
        let pattern = pattern.clone();
        move |reference: Result<refdb::Ref<Oid>, Db::IterError>| {
            let reference = reference.map_err(|err| Tracked::Iter {
                pattern: pattern.clone(),
                source: err.into(),
            })?;

            // We may have seen this config already
            if let Some(config) = seen.get(&reference.target) {
                return Ok(Some(from_reference(&reference.name, config.clone())));
            }

            // Otherwise we attempt to fetch it from the backend
            match db
                .find_config(&reference.target)
                .map_err(|err| Tracked::FindObj {
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

    Ok(db
        .references(&pattern)
        .map_err(|err| Tracked::References {
            pattern: pattern.clone(),
            source: err.into(),
        })?
        .into_iter()
        .filter_map(move |r| resolve(r).transpose()))
}

pub fn tracked_peers<'a, Db>(
    db: &'a Db,
    filter_by: Option<&'_ Urn<Oid>>,
) -> Result<impl Iterator<Item = Result<PeerId, error::Tracked>> + 'a, error::Tracked>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    use error::Tracked;

    let prefix = reflike!("refs/rad/remotes");
    let pattern = match filter_by {
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
        let pattern = pattern.clone();
        move |reference: Result<refdb::Ref<Oid>, Db::IterError>| -> Result<Option<PeerId>, Tracked> {
            let reference = reference.map_err(|err| Tracked::Iter {
                pattern: pattern.clone(),
                source: err.into(),
            })?;

            Ok(reference.name.remote.into())
        }
    };

    Ok(db
        .references(&pattern)
        .map_err(|err| Tracked::References {
            pattern: pattern.clone(),
            source: err.into(),
        })?
        .into_iter()
        .filter_map(move |r| resolve(r).transpose()))
}

pub fn get<Db>(
    db: &Db,
    urn: &'_ Urn<Oid>,
    peer: Option<PeerId>,
) -> Result<Option<Tracked<Oid, config::Config>>, error::Get>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    use error::Get;

    let name = ReferenceName::borrowed(urn, peer);
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

pub fn is_tracked<Db>(db: &Db, urn: &'_ Urn<Oid>, peer: Option<PeerId>) -> Result<bool, error::Get>
where
    Db: refdb::Read<Oid = Oid>,
{
    use error::Get;

    let name = ReferenceName::borrowed(urn, peer);
    match db.find_reference(&name).map_err(|err| Get::FindRef {
        reference: name.into_owned(),
        source: err.into(),
    })? {
        None => Ok(false),
        Some(_) => Ok(true),
    }
}

fn from_reference(
    reference: &ReferenceName<'_, Oid>,
    config: config::Config,
) -> Tracked<Oid, config::Config> {
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

fn load_config<Db>(
    db: &Db,
    reference: &ReferenceName<'_, Oid>,
) -> Result<Option<config::Config>, error::Blob>
where
    Db: refdb::Read<Oid = Oid> + odb::Read<Oid = Oid>,
{
    use error::Blob;

    match db.find_reference(reference).map_err(|err| Blob::FindRef {
        reference: reference.clone().into_owned(),
        source: err.into(),
    })? {
        None => Ok(None),
        Some(r) => Ok(db.find_config(&r.target).map_err(|err| Blob::FindObj {
            reference: r.name.into_owned(),
            target: r.target,
            source: err.into(),
        })?),
    }
}
