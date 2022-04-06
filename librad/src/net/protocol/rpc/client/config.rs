// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    net::{protocol::replication, Network},
    paths::Paths,
};

#[derive(Clone)]
pub struct Config<Signer> {
    pub signer: Signer,
    pub paths: Paths,
    pub replication: replication::Config,
    pub user_storage: UserStorage,
    pub network: Network,
}

#[derive(Clone, Debug)]
pub struct UserStorage {
    pub pool_size: usize,
}

impl Default for UserStorage {
    fn default() -> Self {
        Self {
            pool_size: num_cpus::get_physical(),
        }
    }
}
