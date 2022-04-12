// Copyright © 2021 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::fmt;

use super::error;

pub(crate) struct Progress(String);

impl fmt::Display for Progress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Progress {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for Progress {
    fn from(s: String) -> Self {
        Self(s)
    }
}

pub(crate) trait ProgressReporter {
    type Error;
    fn report(&mut self, progress: Progress)
        -> futures::future::BoxFuture<Result<(), Self::Error>>;
}

pub(crate) async fn report<
    E: std::error::Error + Send + 'static,
    P: ProgressReporter<Error = E>,
>(
    reporter: &mut P,
    msg: impl Into<Progress>,
) -> Result<(), error::Progress<E>> {
    reporter.report(msg.into()).await.map_err(error::Progress)
}
