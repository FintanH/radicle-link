// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::io;

use node_lib::seed::store;

use crate::tempdir::WithTmpDir;

pub type TmpKVStore = WithTmpDir<store::KVStore>;

pub fn kv_store() -> TmpKVStore {
    WithTmpDir::new(|path| -> Result<_, io::Error> {
        Ok(store::KVStore::new(path.join("seeds")).unwrap())
    })
    .unwrap()
}
