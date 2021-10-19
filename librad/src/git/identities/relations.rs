// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::TryFrom as _;

use either::Either;
use serde::{Deserialize, Serialize};

use crate::{
    git::{
        identities,
        refs::{stored, Refs},
        storage::{self, ReadOnlyStorage as _},
        tracking,
        types::{Namespace, Reference},
        Urn,
    },
    identities::{
        relations::{Peer, Status},
        Person,
        SomeIdentity,
    },
    PeerId,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Identities(#[from] identities::Error),
    #[error(transparent)]
    Storage(#[from] storage::Error),
    #[error(transparent)]
    Stored(#[from] stored::Error),
    #[error(transparent)]
    Tracking(#[from] tracking::Error),
    #[error("the identity `{0}` found is not recognised/supported")]
    UknownIdentity(Urn),
}

/// The `rad/self` under a `Project`/`Person`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona<P> {
    /// Generally the [`Person`] data found at `rad/self`.
    person: P,
    /// If the peer is a delegate, and which `PeerId` they are using for this
    /// delegation.
    ///
    /// This field being set indicates that the peer has a significant role in
    /// the `Project` or `Person`. This role can be analogised to the term
    /// "maintainer".
    delegation: Option<PeerId>,
    /// The [`Refs`] the peer is advertising.
    ///
    /// This field being set indicates that the peer has a possible interest in
    /// viewing and editing code collaboration artifacts located in this
    /// `Project` or `Person`.
    refs: Option<Refs>,
}

impl<P> Persona<P> {
    pub fn new(person: P) -> Self {
        Self {
            person,
            delegation: None,
            refs: None,
        }
    }

    pub fn map<Q>(self, f: impl FnOnce(P) -> Q) -> Persona<Q> {
        Persona {
            person: f(self.person),
            delegation: self.delegation,
            refs: self.refs,
        }
    }

    pub fn person(&self) -> &P {
        &self.person
    }

    pub fn delegation(&self) -> Option<PeerId> {
        self.delegation
    }

    pub fn refs(&self) -> Option<&Refs> {
        self.refs.as_ref()
    }
}

/// Determine the [`Persona`] for [`SomeIdentity`] and [`PeerId`].
///
/// If `peer` is `Either::Left` then we have the local `PeerId` and we can
/// ignore it for looking at `rad/signed_refs`.
///
/// If `peer` is `Either::Right` then it is a remote peer and we use it for
/// looking at `refs/<remote>/rad/signed_refs`.
pub fn persona<S>(
    storage: &S,
    person: Person,
    identity: &SomeIdentity,
    peer: Either<PeerId, PeerId>,
) -> Result<Persona<Person>, Error>
where
    S: AsRef<storage::ReadOnly>,
{
    let storage = storage.as_ref();
    let mut persona = Persona::new(person);
    let urn = identity.urn();
    persona.delegation = is_delegate(identity, &urn, peer.into_inner())?;
    persona.refs = Refs::load(storage, &urn, peer.right())?;
    Ok(persona)
}

fn is_delegate(identity: &SomeIdentity, urn: &Urn, peer: PeerId) -> Result<Option<PeerId>, Error> {
    match identity {
        SomeIdentity::Project(ref project) => {
            if project.delegations().owner(peer.as_public_key()).is_some() {
                Ok(Some(peer))
            } else {
                Ok(None)
            }
        },
        SomeIdentity::Person(ref person) => {
            if person.delegations().contains(peer.as_public_key()) {
                Ok(Some(peer))
            } else {
                Ok(None)
            }
        },
        _ => Err(Error::UknownIdentity(urn.clone())),
    }
}

pub type Tracked<P> = Vec<Peer<Status<Persona<P>>>>;

/// Builds the list of tracked peers determining their relation to the `urn`
/// provided.
///
/// If the peer is in the tracking graph but there is no `rad/self` under the
/// tree of remotes, then they have not been replicated, signified by
/// [`Status::NotReplicated`].
///
/// If their `rad/self` is under the tree of remotes, then they have been
/// replicated, signified by [`Status::Replicated`].
pub fn tracked<S>(storage: &S, urn: &Urn) -> Result<Tracked<Person>, Error>
where
    S: AsRef<storage::ReadOnly>,
{
    let storage = storage.as_ref();
    let identity = identities::any::get(storage, urn)?
        .ok_or_else(|| identities::Error::NotFound(urn.clone()))?;

    let mut peers = vec![];

    for peer_id in tracking::tracked(storage, urn)? {
        let rad_self = Urn::try_from(Reference::rad_self(Namespace::from(urn.clone()), peer_id))
            .expect("namespace is set");
        let status = if storage.has_urn(&rad_self)? {
            let malkovich = identities::person::get(storage, &rad_self)?
                .ok_or(identities::Error::NotFound(rad_self))?;

            let persona = persona(storage, malkovich, &identity, Either::Right(peer_id))?;
            Status::replicated(persona)
        } else {
            Status::NotReplicated
        };

        peers.push(Peer::Remote { peer_id, status });
    }

    Ok(peers)
}
