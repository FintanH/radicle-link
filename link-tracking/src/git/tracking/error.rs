// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use radicle_git_ext::{Oid, RefspecPattern};

use thiserror::Error;

use super::RefName;

#[derive(Debug, Error)]
pub enum Batch {
    #[error("failed to find `{reference}` during batch")]
    FindRef {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to write new config to `{reference}` during batch")]
    WriteObj {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed during batch tracking updates")]
    Txn {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Track {
    #[error("failed to create reference `{reference}` during track")]
    Create {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to see if `{reference}` exists during track")]
    FindObj {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to write new config to `{reference}` during track")]
    WriteObj {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Untrack {
    #[error("failed to find config for `{reference}` during untrack")]
    FindObj {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to remove config for `{reference}` during untrack")]
    Delete {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum UntrackAll {
    #[error("failed to get entries for `{spec}` during untrack all")]
    References {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to unpack a reference for `{spec}` during untrack all")]
    Iter {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to remove configs for `{spec}` during untrack all")]
    Delete {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Update {
    #[error("failed to find `{reference}` during update")]
    FindRef {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to write new config object for `{reference}` during update")]
    WriteObj {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to point `{reference}` to new object `{object}` during update")]
    WriteRef {
        object: Oid,
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Tracked {
    #[error("failed to get object for `{reference}`@`{target}` while getting tracked entries")]
    FindObj {
        reference: RefName<'static, Oid>,
        target: Oid,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to unpack a reference entry while getting tracked entries for `{spec}`")]
    Iter {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed getting tracked entries for `{spec}`")]
    References {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum TrackedPeers {
    #[error("failed to unpack a reference entry while getting tracked entries for `{spec}`")]
    Iter {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed getting tracked entries for `{spec}`")]
    References {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Get {
    #[error("failed to get object for `{reference}`@`{target}` while getting entry")]
    FindObj {
        reference: RefName<'static, Oid>,
        target: Oid,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to find `{reference}` during get")]
    FindRef {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum IsTracked {
    #[error("failed to find `{reference}` during get")]
    FindRef {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum DefaultOnly {
    #[error("failed to unpack a reference entry while getting tracked entries for `{spec}`")]
    Iter {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed getting tracked entries for `{spec}`")]
    References {
        spec: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Config {
    #[error("failed to get object for `{reference}@{target}` while loading blob")]
    FindObj {
        reference: RefName<'static, Oid>,
        target: Oid,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to find `{reference}` while loading blob")]
    FindRef {
        reference: RefName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}
