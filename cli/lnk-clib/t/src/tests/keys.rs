// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use tempfile::tempdir;

use it_helpers::ssh::{with_async_ssh_agent, with_ssh_agent};
use librad::{
    crypto::{
        keystore::{
            crypto::{Pwhash, KDF_PARAMS_TEST},
            pinentry::SecUtf8,
            sign::Signer as _,
            Keystore as _,
        },
        SecretKey,
    },
    git::storage::Storage,
    profile::{LnkHome, Profile, ProfileId},
    PeerId,
    Signer as _,
};
use lnk_clib::keys::{file_storage, ssh};
use test_helpers::logging;

#[test]
fn agent_signature() -> anyhow::Result<()> {
    logging::init();

    let temp = tempdir()?;
    let pass = Pwhash::new(SecUtf8::from(b"42".to_vec()), *KDF_PARAMS_TEST);
    let home = LnkHome::Root(temp.path().to_path_buf());
    let id = ProfileId::new();
    let profile = Profile::from_home(&home, Some(id))?;
    let key = SecretKey::new();
    let mut key_store = file_storage(&profile, pass.clone());
    key_store.put_key(key.clone())?;
    let _ = Storage::open(profile.paths(), key)?;

    let (sig, peer_id) = with_ssh_agent(|sock| {
        ssh::add_signer(&profile, sock.clone(), pass, &[])?;
        let signer = ssh::signer(&profile, sock)?;
        let sig = signer.sign_blocking(b"secret message")?;
        Ok((sig, signer.peer_id()))
    })?;

    let pk = peer_id.as_public_key();
    assert!(pk.verify(&sig.into(), b"secret message"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn async_agent_signature() -> anyhow::Result<()> {
    logging::init();

    let temp = tempdir()?;
    let pass = Pwhash::new(SecUtf8::from(b"42".to_vec()), *KDF_PARAMS_TEST);
    let home = LnkHome::Root(temp.path().to_path_buf());
    let id = ProfileId::new();
    let profile = Profile::from_home(&home, Some(id))?;
    let key = SecretKey::new();
    let pk = PeerId::from(key.clone());
    let pk = pk.as_public_key();
    let mut key_store = file_storage(&profile, pass.clone());
    key_store.put_key(key.clone())?;
    let _ = Storage::open(profile.paths(), key)?;

    let sig = with_async_ssh_agent(|sock| async {
        ssh::async_add_signer(&profile, sock.clone(), pass, &[]).await?;
        let signer = ssh::async_signer(&profile, sock).await?;
        Ok(signer.sign_blocking(b"secret message")?)
    })
    .await?;

    assert!(pk.verify(&sig.into(), b"secret message"));

    Ok(())
}
