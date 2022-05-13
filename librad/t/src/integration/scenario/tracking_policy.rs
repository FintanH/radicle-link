// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    convert::{TryFrom, TryInto},
    ops::Index as _,
};

use blocking::unblock;
use git_ref_format::{lit, RefString};
use it_helpers::{
    fixed::{TestPerson, TestProject},
    git::create_commit,
    testnet,
};
use librad::{
    self,
    git::{
        local::url::LocalUrl,
        refs::Refs,
        storage::ReadOnlyStorage as _,
        tracking,
        types::{remote, Flat, Force, GenericRef, Namespace, Reference, Refspec, Remote},
    },
    git_ext as ext,
    reflike,
    refspec_pattern,
};
use test_helpers::logging;

fn config() -> testnet::Config {
    testnet::Config {
        num_peers: nonzero!(3usize),
        min_connected: 3,
        bootstrap: testnet::Bootstrap::from_env(),
    }
}

#[test]
fn cannot_ignore_delegate() {
    logging::init();

    let net = testnet::run(config()).unwrap();
    net.enter(async {
        let peer1 = net.peers().index(0);
        let peer2 = net.peers().index(1);

        let proj = peer1
            .using_storage(TestProject::create)
            .await
            .unwrap()
            .unwrap();
        proj.pull(peer1, peer2).await.unwrap();
        peer2
            .using_storage({
                let peer1_id = peer1.peer_id();
                let urn = proj.project.urn();
                move |storage| -> anyhow::Result<()> {
                    assert!(tracking::track(
                        storage,
                        &urn,
                        Some(peer1_id),
                        tracking::Config {
                            data: false,
                            ..tracking::Config::default()
                        },
                        tracking::policy::Track::Any,
                    )?
                    .is_ok());
                    Refs::update(storage, &urn)?;
                    Ok(())
                }
            })
            .await
            .unwrap()
            .unwrap();

        let default_branch: RefString = proj
            .project
            .doc
            .payload
            .subject
            .default_branch
            .as_ref()
            .map(|cstring| cstring.to_string())
            .unwrap_or_else(|| "mistress".to_owned())
            .try_into()
            .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let commit_id = unblock({
            let urn = proj.project.urn();
            let owner_subject = proj.owner.subject().clone();
            let default_branch = default_branch.clone();
            let peer1 = (*peer1).clone();
            move || {
                // Perform commit and push to working copy on peer1
                let repo = git2::Repository::init(tmp.path().join("peer1")).unwrap();
                let url = LocalUrl::from(urn.clone());
                let heads = Reference::heads(Namespace::from(urn), Some(peer1.peer_id()));
                let remotes = GenericRef::heads(
                    Flat,
                    ext::RefLike::try_from(format!("{}@{}", owner_subject.name, peer1.peer_id()))
                        .unwrap(),
                );
                let mastor = lit::refs_heads(default_branch).into();
                let mut remote = Remote::rad_remote(
                    url,
                    Refspec {
                        src: &remotes,
                        dst: &heads,
                        force: Force::True,
                    },
                );
                let oid = create_commit(&repo, mastor).unwrap();
                let updated = remote
                    .push(
                        peer1,
                        &repo,
                        remote::LocalPushspec::Matching {
                            pattern: refspec_pattern!("refs/heads/*"),
                            force: Force::True,
                        },
                    )
                    .unwrap()
                    .collect::<Vec<_>>();
                debug!("push updated refs: {:?}", updated);

                ext::Oid::from(oid)
            }
        })
        .await;

        let expected_urn = proj.project.urn().with_path(
            reflike!("refs/remotes")
                .join(peer1.peer_id())
                .join(reflike!("heads"))
                .join(&default_branch),
        );

        proj.pull(peer1, peer2).await.unwrap();

        let has_commit = peer2
            .using_storage({
                let urn = expected_urn.clone();
                move |storage| -> anyhow::Result<bool> {
                    let has = storage.has_commit(&urn, commit_id)?;
                    Ok(has)
                }
            })
            .await
            .unwrap()
            .unwrap();

        assert!(
            has_commit,
            "peer 2 missing commit `{}@{}`",
            expected_urn, commit_id
        );
    })
}

#[test]
fn ignore_tracking() {
    logging::init();

    let net = testnet::run(config()).unwrap();
    net.enter(async {
        let peer1 = net.peers().index(0);
        let peer2 = net.peers().index(1);

        let proj = peer2
            .using_storage(TestProject::create)
            .await
            .unwrap()
            .unwrap();
        proj.pull(peer1, peer2).await.unwrap();
        peer1
            .using_storage({
                let peer2_id = peer2.peer_id();
                let urn = proj.project.urn();
                move |storage| -> anyhow::Result<()> {
                    assert!(tracking::track(
                        storage,
                        &urn,
                        Some(peer2_id),
                        tracking::Config {
                            data: false,
                            ..tracking::Config::default()
                        },
                        tracking::policy::Track::Any,
                    )?
                    .is_ok());
                    Refs::update(storage, &urn)?;
                    Ok(())
                }
            })
            .await
            .unwrap()
            .unwrap();
        let pers = peer2
            .using_storage(move |storage| -> anyhow::Result<TestPerson> {
                let person = TestPerson::create(&storage)?;
                let local = person.local(&storage)?;
                storage.config()?.set_user(local)?;
                Ok(person)
            })
            .await
            .unwrap()
            .unwrap();
        let default_branch: RefString = proj
            .project
            .doc
            .payload
            .subject
            .default_branch
            .as_ref()
            .map(|cstring| cstring.to_string())
            .unwrap_or_else(|| "mistress".to_owned())
            .try_into()
            .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let commit_id = unblock({
            let urn = proj.project.urn();
            let owner_subject = pers.owner.subject().clone();
            let default_branch = default_branch.clone();
            let peer2 = (*peer2).clone();
            move || {
                // Perform commit and push to working copy on peer1
                let repo = git2::Repository::init(tmp.path().join("peer2")).unwrap();
                let url = LocalUrl::from(urn.clone());
                let heads = Reference::heads(Namespace::from(urn), Some(peer2.peer_id()));
                let remotes = GenericRef::heads(
                    Flat,
                    ext::RefLike::try_from(format!("{}@{}", "peer2", owner_subject.name)).unwrap(),
                );
                let mastor = lit::refs_heads(default_branch).into();
                let mut remote = Remote::rad_remote(
                    url,
                    Refspec {
                        src: &remotes,
                        dst: &heads,
                        force: Force::True,
                    },
                );
                let oid = create_commit(&repo, mastor).unwrap();
                let updated = remote
                    .push(
                        peer2,
                        &repo,
                        remote::LocalPushspec::Matching {
                            pattern: refspec_pattern!("refs/heads/*"),
                            force: Force::True,
                        },
                    )
                    .unwrap()
                    .collect::<Vec<_>>();
                debug!("push updated refs: {:?}", updated);

                ext::Oid::from(oid)
            }
        })
        .await;

        let expected_urn = proj.project.urn().with_path(
            reflike!("refs/remotes")
                .join(peer2.peer_id())
                .join(reflike!("heads"))
                .join(&default_branch),
        );

        proj.pull(peer2, peer1).await.unwrap();

        let has_commit = peer2
            .using_storage({
                let urn = expected_urn.clone();
                move |storage| -> anyhow::Result<bool> {
                    let has = storage.has_commit(&urn, commit_id)?;
                    Ok(has)
                }
            })
            .await
            .unwrap()
            .unwrap();

        assert!(
            !has_commit,
            "peer 1 has commit `{}@{}`, but it should be ignored",
            expected_urn, commit_id
        );
    })
}

/// `peer1` is a delegate of a project and tracks `peer2`.
/// When `peer3` replicates from `peer1` they should have references for `peer1`
/// and `peer2`, due to the tracking graph.
#[test]
fn ignore_transitive_tracking() {
    logging::init();

    let net = testnet::run(config()).unwrap();
    net.enter(async {
        let peer1 = net.peers().index(0);
        let peer2 = net.peers().index(1);
        let peer3 = net.peers().index(2);

        for x in 0..=2 {
            info!("peer{}: {}", x + 1, net.peers().index(x).peer_id())
        }

        let proj = peer1
            .using_storage(TestProject::create)
            .await
            .unwrap()
            .unwrap();

        peer1
            .using_storage({
                let peer2_id = peer2.peer_id();
                let urn = proj.project.urn();
                move |storage| -> anyhow::Result<()> {
                    assert!(tracking::track(
                        storage,
                        &urn,
                        Some(peer2_id),
                        tracking::Config {
                            data: false,
                            cobs: tracking::config::cobs::Cobs::deny_all(),
                        },
                        tracking::policy::Track::Any,
                    )?
                    .is_ok());
                    Refs::update(storage, &urn)?;
                    Ok(())
                }
            })
            .await
            .unwrap()
            .unwrap();

        debug!("pull from peer1 to peer2");
        proj.pull(peer1, peer2).await.unwrap();
        debug!("pull from peer2 to peer1");
        proj.pull(peer2, peer1).await.unwrap();
        debug!("pull from peer1 to peer3");
        proj.pull(peer1, peer3).await.unwrap();

        peer1
            .using_storage({
                move |storage| {
                    let names = storage
                        .reference_names(&refspec_pattern!("refs/namespaces/*"))
                        .unwrap();
                    for name in names {
                        println!("reference: {}", name.unwrap());
                    }
                }
            })
            .await
            .unwrap();
        let has_rad_id = peer1
            .using_storage({
                let urn = proj.project.urn();
                let remote = peer2.peer_id();
                move |storage| -> anyhow::Result<bool> {
                    let rad_id =
                        Reference::rad_id(Namespace::from(urn.clone())).with_remote(remote);
                    let has = storage.has_ref(&rad_id)?;
                    Ok(has)
                }
            })
            .await
            .unwrap()
            .unwrap();

        assert!(
            has_rad_id,
            "peer 1 missing `refs/remotes/{}/rad/id`",
            peer2.peer_id()
        );

        // Has peer1 refs?
        let has_rad_id = peer3
            .using_storage({
                let urn = proj.project.urn();
                let delegate = proj.owner.urn();
                let remote = peer1.peer_id();
                move |storage| -> anyhow::Result<bool> {
                    let rad_id =
                        Reference::rad_id(Namespace::from(urn.clone())).with_remote(remote);
                    let has = storage.has_ref(&rad_id)?;
                    Ok(has)
                }
            })
            .await
            .unwrap()
            .unwrap();

        assert!(
            has_rad_id,
            "peer 3 missing `refs/remotes/{}/rad/id`",
            peer1.peer_id()
        );

        let has_rad_id = peer3
            .using_storage({
                let urn = proj.project.urn();
                let delegate = proj.owner.urn();
                let remote = peer2.peer_id();
                move |storage| -> anyhow::Result<bool> {
                    // let spec = refspec_pattern!("refs/namespaces/*");
                    // for r in storage.reference_names(&spec).unwrap() {
                    //     println!("peer3 ref: {:?}", r);
                    // }
                    let rad_id =
                        Reference::rad_id(Namespace::from(urn.clone())).with_remote(remote);
                    let has = storage.has_ref(&rad_id)?;
                    Ok(has)
                }
            })
            .await
            .unwrap()
            .unwrap();

        assert!(
            has_rad_id,
            "peer 3 missing `refs/remotes/{}/rad/id`",
            peer2.peer_id()
        );
    })
}
