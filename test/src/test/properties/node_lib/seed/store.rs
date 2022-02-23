// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::collections::BTreeSet;

use proptest::{collection, prelude::*};

use node_lib::seed::{
    store::{Scan as _, Store},
    Seed,
};

use crate::node_lib::seed::{gen_seed, store::kv_store, Host};

proptest! {
    #[test]
    fn roundtrip(seed in gen_seed()) {
        let mut store = kv_store();
        store.insert(seed.clone()).unwrap();
        assert_eq!(store.get(seed.peer).unwrap(), Some(seed));
    }

    #[test]
    fn update(seed in gen_seed()) {
        let mut store = kv_store();
        store.insert(seed.clone()).unwrap();
        let updated = store
            .update(seed.peer, |old: &mut Host| {
                *old = Host {
                    addr: old.addr.clone(),
                    port: old.port.checked_add(1).unwrap_or(0),
                }
            })
            .unwrap();

        let new_seed = Seed {
            peer: seed.peer,
            addrs: Host {
                addr: seed.addrs.addr,
                port: seed.addrs.port.checked_add(1).unwrap_or(0)
            }
        };
        if updated {
            assert_eq!(store.get(seed.peer).unwrap(), Some(new_seed));
        }
    }

    #[test]
    fn insert_is_idempotent(seed in gen_seed()) {
        let mut store = kv_store();
        store.insert(seed.clone()).unwrap();
        store.insert(seed.clone()).unwrap();
            assert_eq!(store.get(seed.peer).unwrap(), Some(seed));
    }

    #[test]
    fn remove_is_idempotent(seed in gen_seed()) {
        let mut store = kv_store();
        store.insert(seed.clone()).unwrap();

        // TODO: type checker can't figure out the generic parameter here
        assert!(Store::<Seed<Host>>::remove(&mut *store, seed.peer).unwrap());
        assert_eq!(Store::<Seed<Host>>::get(&*store, seed.peer).unwrap(), None);
        assert!(!Store::<Seed<Host>>::remove(&mut *store, seed.peer).unwrap());
        assert_eq!(Store::<Seed<Host>>::get(&*store, seed.peer).unwrap(), None);
    }

    #[test]
    fn read(seeds in collection::vec(gen_seed(), 1..5)) {
        let mut store = kv_store();

        for seed in &seeds {
            store.insert(seed.clone()).unwrap();
        }

        let seeds = seeds.into_iter().collect::<BTreeSet<_>>();
        assert_eq!(
            store
                .scan()
                .unwrap()
                .collect::<Result<BTreeSet<_>, _>>()
                .unwrap(),
            seeds
        );
}

}
