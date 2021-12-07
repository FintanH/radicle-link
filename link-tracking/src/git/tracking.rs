// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::{TryFrom, TryInto as _};

use tracing::warn;

use link_canonical::Canonical as _;
use link_crypto::PeerId;
use link_identities::urn::Urn;
use radicle_git_ext::{Oid, RefLike};

use crate::tracking::Tracked;

use super::{config, odb, refdb};

pub mod error;
pub mod reference;
pub use reference::{Reference, ReferenceRef, Remote};

pub fn track<Db>(
    db: &Db,
    urn: &Urn<Oid>,
    peer: Option<PeerId>,
    config: Option<config::Config>,
) -> Result<bool, error::Track>
where
    Db: odb::Read<Oid = Oid>
        + odb::Write<Oid = Oid>
        + refdb::Read<Oid = Oid>
        + refdb::Write<Oid = Oid>,
{
    use error::Track;

    let reference = ReferenceRef::new(urn, peer);
    let mk_ref = || reference.into_owned();
    match blob(db, &reference).map_err(|err| Track::FindObj {
        reference: mk_ref(),
        source: err.into(),
    })? {
        None => {
            let bytes = config.unwrap_or_default().canonical_form().unwrap();
            let target = db.write_object(bytes).map_err(|err| Track::WriteObj {
                reference: mk_ref(),
                source: err.into(),
            })?;
            db.create(&reference, target)
                .map(|_| true)
                .map_err(|err| Track::Create {
                    reference: mk_ref(),
                    source: err.into(),
                })
        },
        Some(_) => Ok(false),
    }
}

pub fn untrack<Db>(db: &Db, urn: &Urn<Oid>, peer: Option<PeerId>) -> Result<bool, error::Untrack>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
{
    use error::Untrack;

    let reference = ReferenceRef::new(urn, peer);
    let mk_ref = || reference.into_owned();
    match blob(db, &reference).map_err(|err| Untrack::FindObj {
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

pub fn update<Db>(
    db: &Db,
    urn: &Urn<Oid>,
    peer: Option<PeerId>,
    config: config::Config,
) -> Result<bool, error::Update>
where
    Db: odb::Write<Oid = Oid> + refdb::Read<Oid = Oid> + refdb::Write<Oid = Oid>,
{
    use error::Update;

    let name = ReferenceRef::new(urn, peer);
    let mk_ref = || name.into_owned();
    match db.find_reference(&name).map_err(|err| Update::FindRef {
        reference: mk_ref(),
        source: err.into(),
    })? {
        None => Ok(false),
        Some(reference) => {
            let bytes = config.canonical_form().unwrap();
            let oid = db.write_object(bytes).map_err(|err| Update::WriteObj {
                reference: mk_ref(),
                source: err.into(),
            })?;
            db.write_target(&reference.name.as_ref(), oid)
                .map_err(|err| Update::WriteRef {
                    object: oid,
                    reference: mk_ref(),
                    source: err.into(),
                })?;
            Ok(true)
        },
    }
}

pub fn tracked<Db>(
    db: &Db,
    filter_by: Option<&'_ Urn<Oid>>,
) -> Result<Vec<Tracked<Oid, config::Config>>, error::Tracked>
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

    let mut references = vec![];

    for reference in db.references(&pattern).map_err(|err| Tracked::References {
        pattern: pattern.clone(),
        source: err.into(),
    })? {
        let reference = reference.map_err(|err| Tracked::Iter {
            pattern: pattern.clone(),
            source: err.into(),
        })?;
        match db
            .find_blob(&reference.target)
            .map_err(|err| Tracked::FindObj {
                reference: reference.name.clone(),
                target: reference.target,
                source: err.into(),
            })? {
            None => {
                warn!(name=?reference.name, oid=?reference.target, "missing blob")
            },
            Some(obj) => {
                let config: config::Config = obj.try_into().map_err(|err| Tracked::Config {
                    reference: reference.name.clone(),
                    target: reference.target,
                    source: err.into(),
                })?;
                references.push(from_reference(&reference.name.as_ref(), config));
            },
        }
    }

    Ok(references)
}

pub fn tracked_peers<Db>(
    db: &Db,
    filter_by: Option<&'_ Urn<Oid>>,
) -> Result<impl Iterator<Item = PeerId>, error::Tracked>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    Ok(tracked(db, filter_by)?
        .into_iter()
        .filter_map(|tracked| tracked.peer_id()))
}

pub fn get<Db>(
    db: &Db,
    urn: &Urn<Oid>,
    peer: Option<PeerId>,
) -> Result<Option<Tracked<Oid, config::Config>>, error::Get>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    use error::Get;

    let name = ReferenceRef::new(urn, peer);
    match db.find_reference(&name).map_err(|err| Get::FindRef {
        reference: name.into_owned(),
        source: err.into(),
    })? {
        None => Ok(None),
        Some(reference) => match db
            .find_blob(&reference.target)
            .map_err(|err| Get::FindObj {
                reference: reference.name.clone(),
                target: reference.target,
                source: err.into(),
            })? {
            None => Ok(None),
            Some(obj) => {
                let config: config::Config = obj.try_into().map_err(|err| Get::Config {
                    reference: reference.name,
                    target: reference.target,
                    source: err.into(),
                })?;
                Ok(Some(from_reference(&name, config)))
            },
        },
    }
}

pub fn is_tracked<Db>(
    backend: &Db,
    urn: &Urn<Oid>,
    peer: Option<PeerId>,
) -> Result<bool, error::Get>
where
    Db: odb::Read<Oid = Oid> + refdb::Read<Oid = Oid>,
{
    get(backend, urn, peer).map(|tracked| tracked.is_some())
}

fn from_reference(
    reference: &ReferenceRef<'_, Oid>,
    config: config::Config,
) -> Tracked<Oid, config::Config> {
    match reference.remote {
        Remote::Default => Tracked::Default {
            urn: reference.urn.clone(),
            config,
        },
        Remote::Peer(peer) => Tracked::Peer {
            urn: reference.urn.clone(),
            peer,
            config,
        },
    }
}

fn blob<Db>(db: &Db, reference: &ReferenceRef<'_, Oid>) -> Result<Option<Db::Blob>, error::Blob>
where
    Db: refdb::Read<Oid = Oid> + odb::Read<Oid = Oid>,
{
    use error::Blob;

    match db.find_reference(reference).map_err(|err| Blob::FindRef {
        reference: reference.into_owned(),
        source: err.into(),
    })? {
        None => Ok(None),
        Some(r) => Ok(db.find_blob(&r.target).map_err(|err| Blob::FindObj {
            reference: r.name,
            target: r.target,
            source: err.into(),
        })?),
    }
}
