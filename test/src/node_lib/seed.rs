// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

pub mod store;

use proptest::prelude::*;
use serde::{Deserialize, Serialize};

use node_lib::Seed;

use crate::librad::peer::gen_peer_id;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Host {
    pub addr: String,
    pub port: u8,
}

pub fn gen_seed() -> impl Strategy<Value = Seed<Host>> {
    gen_peer_id().prop_flat_map(move |peer| {
        any::<u8>().prop_map(move |port| Seed {
            peer,
            addrs: Host {
                addr: "localhost".to_string(),
                port,
            },
        })
    })
}
