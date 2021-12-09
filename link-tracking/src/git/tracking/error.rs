// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use radicle_git_ext::{Oid, RefspecPattern};

use thiserror::Error;

use super::ReferenceName;

#[derive(Debug, Error)]
pub enum Track {
    #[error("failed to create reference `{reference}` during track")]
    Create {
        reference: ReferenceName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to see if `{reference}` exists during track")]
    FindObj {
        reference: ReferenceName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to write new config to `{reference}` during track")]
    WriteObj {
        reference: ReferenceName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Untrack<'a> {
    #[error("failed to find config for `{reference}` during untrack")]
    FindObj {
        reference: ReferenceName<'a, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to remove config for `{reference}` during untrack")]
    Delete {
        reference: ReferenceName<'a, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Update<'a> {
    #[error("failed to find `{reference}` during update")]
    FindRef {
        reference: ReferenceName<'a, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to write new config object for `{reference}` during update")]
    WriteObj {
        reference: ReferenceName<'a, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to point `{reference}` to new object `{object}` during update")]
    WriteRef {
        object: Oid,
        reference: ReferenceName<'a, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Tracked {
    #[error("failed to get object for `{reference}`@`{target}` while getting tracked entries")]
    FindObj {
        reference: ReferenceName<'static, Oid>,
        target: Oid,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to unpack a reference entry while getting tracked entries for `{pattern}`")]
    Iter {
        pattern: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed getting tracked entries for `{pattern}`")]
    References {
        pattern: RefspecPattern,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Get {
    #[error("failed to get object for `{reference}`@`{target}` while getting entry")]
    FindObj {
        reference: ReferenceName<'static, Oid>,
        target: Oid,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to find `{reference}` during get")]
    FindRef {
        reference: ReferenceName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug, Error)]
pub enum Blob {
    #[error("failed to get object for `{reference}@{target}` while loading blob")]
    FindObj {
        reference: ReferenceName<'static, Oid>,
        target: Oid,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error("failed to find `{reference}` while loading blob")]
    FindRef {
        reference: ReferenceName<'static, Oid>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}
