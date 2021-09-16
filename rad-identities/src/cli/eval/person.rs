// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{convert::TryFrom as _, path::PathBuf};

use anyhow::anyhow;

use librad::{
    crypto::PublicKey,
    git::{
        identities,
        types::{Namespace, Reference},
        Urn,
    },
    identities::payload,
    profile::Profile,
    PeerId,
};
use rad_clib::storage::{self, ssh};

use crate::{cli::args::person::*, person};

pub fn eval(profile: &Profile, opts: Options) -> anyhow::Result<()> {
    match opts {
        Options::Create(CreateOptions { create }) => eval_create(profile, create)?,
        Options::Get(Get { urn, peer }) => eval_get(profile, urn, peer)?,
        Options::List(List {}) => eval_list(profile)?,
        Options::Update(Update {
            urn,
            whoami,
            payload,
            ext,
            delegations,
        }) => eval_update(profile, urn, whoami, payload, ext, delegations)?,
        Options::Checkout(Checkout { urn, path, peer }) => eval_checkout(profile, urn, path, peer)?,
        Options::Review(Review {}) => unimplemented!(),
    }

    Ok(())
}

fn eval_create(profile: &Profile, create: Create) -> anyhow::Result<()> {
    let (signer, storage) = ssh::storage(profile)?;
    let paths = profile.paths();
    let person = match create {
        Create::New(New {
            payload,
            ext,
            delegations,
            path,
        }) => person::create(
            &storage,
            paths.clone(),
            signer,
            payload,
            ext,
            delegations,
            person::Creation::New { path },
        )?,
        Create::Existing(Existing {
            payload,
            ext,
            delegations,
            path,
        }) => person::create(
            &storage,
            paths.clone(),
            signer,
            payload,
            ext,
            delegations,
            person::Creation::Existing { path },
        )?,
    };
    println!("{}", serde_json::to_string(&person.subject())?);
    Ok(())
}

fn eval_get(profile: &Profile, urn: Urn, peer: Option<PeerId>) -> anyhow::Result<()> {
    let storage = storage::read_only(profile)?;
    let rad = Reference::rad_id(Namespace::from(&urn)).with_remote(peer);
    let urn = Urn::try_from(rad).map_err(|err| anyhow!(err))?;
    let project =
        person::get(&storage, &urn)?.ok_or_else(|| identities::Error::NotFound(urn.clone()))?;
    println!("{}", serde_json::to_string(&project.payload())?);
    Ok(())
}

fn eval_list(profile: &Profile) -> anyhow::Result<()> {
    let storage = storage::read_only(profile)?;
    let persons = person::list(&storage)?;
    let persons = persons
        .map(|p| {
            p.map(|p| person::PersonDisplay {
                urn: p.urn(),
                payload: p.payload().clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    println!("{}", serde_json::to_string(&persons)?);
    Ok(())
}

fn eval_update(
    profile: &Profile,
    urn: Urn,
    whoami: Option<Urn>,
    payload: Option<payload::Person>,
    ext: Vec<payload::Ext<serde_json::Value>>,
    delegations: Vec<PublicKey>,
) -> anyhow::Result<()> {
    let (_, storage) = ssh::storage(profile)?;
    let person = person::update(
        &storage,
        &urn,
        whoami,
        payload,
        ext,
        delegations.into_iter(),
    )?;
    println!("{}", serde_json::to_string(person.payload())?);
    Ok(())
}

fn eval_checkout(
    profile: &Profile,
    urn: Urn,
    path: PathBuf,
    peer: Option<PeerId>,
) -> anyhow::Result<()> {
    let paths = profile.paths();
    let (signer, storage) = ssh::storage(profile)?;
    let repo = person::checkout(&storage, paths.clone(), signer, &urn, peer, path)?;
    println!("working copy created at `{}`", repo.path().display());
    Ok(())
}
