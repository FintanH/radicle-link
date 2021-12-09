// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use radicle_git_ext::RefspecPattern;

use crate::git::tracking::reference::ReferenceName;

#[derive(Debug)]
pub struct Ref<'a, Oid: ToOwned + Clone> {
    pub name: ReferenceName<'a, Oid>,
    pub target: Oid,
}

pub trait Read {
    type FindError: std::error::Error + Send + Sync + 'static;
    type ReferencesError: std::error::Error + Send + Sync + 'static;
    type IterError: std::error::Error + Send + Sync + 'static;

    type Oid: Clone;

    fn find_reference(
        &self,
        reference: &ReferenceName<'_, Self::Oid>,
    ) -> Result<Option<Ref<Self::Oid>>, Self::FindError>;

    #[allow(clippy::type_complexity)]
    fn references(
        &self,
        spec: &RefspecPattern,
    ) -> Result<Vec<Result<Ref<Self::Oid>, Self::IterError>>, Self::ReferencesError>;
}

pub trait Write {
    type CreateError: std::error::Error + Send + Sync + 'static;
    type WriteError: std::error::Error + Send + Sync + 'static;
    type DeleteError: std::error::Error + Send + Sync + 'static;

    type Oid: Clone;

    fn create(
        &self,
        name: &ReferenceName<'_, Self::Oid>,
        target: Self::Oid,
    ) -> Result<Ref<Self::Oid>, Self::CreateError>;

    fn write_target(
        &self,
        reference: &ReferenceName<'_, Self::Oid>,
        target: Self::Oid,
    ) -> Result<(), Self::WriteError>;

    fn delete_reference(
        &self,
        reference: &ReferenceName<'_, Self::Oid>,
    ) -> Result<bool, Self::DeleteError>;
}
