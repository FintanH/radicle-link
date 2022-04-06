// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{pin, task};

use futures::{self, Future as _, Stream, StreamExt as _};
use link_async::{JoinError, Task};

use crate::net::protocol::request_pull;

use super::error;

pub(super) struct RequestPull<S> {
    pub responses: S,
    pub replicate: Option<Task<()>>,
}

impl<S> Stream for RequestPull<S>
where
    S: Stream<Item = Result<request_pull::Response, error::RequestPull>> + Unpin,
{
    type Item = Result<request_pull::Response, error::RequestPull>;

    fn poll_next(
        mut self: pin::Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        match &mut self.replicate {
            Some(replicate) => {
                futures::pin_mut!(replicate);
                match replicate.poll(cx) {
                    task::Poll::Ready(Ok(())) => {
                        tracing::trace!("request-pull replication task completed");
                        self.replicate = None;
                    },
                    task::Poll::Ready(Err(JoinError::Cancelled)) => {
                        tracing::warn!("request-pull replication task cancelled")
                    },
                    task::Poll::Ready(Err(JoinError::Panicked(e))) => {
                        tracing::warn!("request-pull replication task panicked");
                        return task::Poll::Ready(Some(Err(error::RequestPull::Panicked(e))));
                    },
                    task::Poll::Pending => {},
                }
            },
            None => {},
        }
        self.responses.poll_next_unpin(cx)
    }
}
