use librad::{
    profile::{ProfileId, RadHome},
    PeerId,
};
use node_lib::seed::{store, Scan as _, Seed, Store};

use crate::get_or_active;

fn store<H, P>(home: H, id: P) -> anyhow::Result<store::KVStore>
where
    H: Into<Option<RadHome>>,
    P: Into<Option<ProfileId>>,
{
    let home = home.into().unwrap_or_default();
    let profile = get_or_active(&home, id)?;
    let paths = profile.paths();
    Ok(store::KVStore::new(store::file::default_path(paths))?)
}

pub fn get<H, P>(home: H, id: P, peer: PeerId) -> anyhow::Result<Option<Seed<String>>>
where
    H: Into<Option<RadHome>>,
    P: Into<Option<ProfileId>>,
{
    let store = store(home, id)?;
    Ok(store.get(peer)?)
}

pub fn add<H, P>(home: H, id: P, peer: PeerId, addrs: String) -> anyhow::Result<Seed<String>>
where
    H: Into<Option<RadHome>>,
    P: Into<Option<ProfileId>>,
{
    let mut store = store(home, id)?;
    if !Store::<Seed<String>>::exists(&store, peer)? {
        let seed = Seed { peer, addrs };
        let _new = store.insert(seed.clone())?;
        debug_assert!(_new.is_none());
        Ok(seed)
    } else {
        Err(anyhow::anyhow!(format!(
            "seed exists for `{}`, perhaps you want to use `set`?",
            peer
        )))
    }
}

pub fn ls<H, P>(home: H, id: P) -> anyhow::Result<Vec<Seed<String>>>
where
    H: Into<Option<RadHome>>,
    P: Into<Option<ProfileId>>,
{
    let store = store(home, id)?;
    Ok(store.scan()?.collect::<Result<_, _>>()?)
}

pub fn rm<H, P>(home: H, id: P, peer: PeerId) -> anyhow::Result<bool>
where
    H: Into<Option<RadHome>>,
    P: Into<Option<ProfileId>>,
{
    let mut store = store(home, id)?;
    Ok(Store::<Seed<String>>::remove(&mut store, peer)?)
}

pub fn set<H, P>(
    home: H,
    id: P,
    peer: PeerId,
    addrs: String,
) -> anyhow::Result<Option<Seed<String>>>
where
    H: Into<Option<RadHome>>,
    P: Into<Option<ProfileId>>,
{
    let mut store = store(home, id)?;
    Ok(store.insert(Seed { peer, addrs })?)
}
