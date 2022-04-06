// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    panic,
    pin::Pin,
    task::{Context, Poll},
};

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

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match &mut self.replicate {
            Some(replicate) => {
                futures::pin_mut!(replicate);
                match replicate.poll(cx) {
                    Poll::Ready(Ok(())) => {
                        tracing::trace!("request-pull replication task completed");
                        self.replicate = None;
                    },
                    Poll::Ready(Err(JoinError::Cancelled)) => {
                        tracing::warn!("request-pull replication task cancelled");
                        return Poll::Ready(Some(Err(error::RequestPull::Cancelled)));
                    },
                    Poll::Ready(Err(JoinError::Panicked(e))) => {
                        tracing::warn!("request-pull replication task panicked");
                        panic::resume_unwind(e)
                    },
                    Poll::Pending => {},
                }
            },
            None => {},
        }
        self.responses.poll_next_unpin(cx)
    }
}
