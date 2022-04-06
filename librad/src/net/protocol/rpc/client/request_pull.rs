// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    net::SocketAddr,
    panic,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{
    self,
    stream::{BoxStream, FuturesUnordered},
    Stream,
    StreamExt as _,
};
use link_async::{JoinError, Spawner, Task};
use tokio::sync::mpsc;

use crate::{
    git::Urn,
    net::{
        protocol::{self, request_pull},
        quic::{self, ConnectPeer},
    },
    paths::Paths,
    PeerId,
};

use super::{error, streams};

/// A series of request-pull responses.
///
/// To get the next response, call [`RequestPull::next`]. The responses will be
/// finished once the next result will either one of the following:
///   * `None` was returned
///   * A successful response, [`request_pull::Response::Success`]
///   * An error response, [`request_pull::Response::Error`]
///   * An error,  [`error::RequestPull`]
pub struct RequestPull {
    pub(super) replies: mpsc::Receiver<Result<request_pull::Response, error::RequestPull>>,
}

impl RequestPull {
    /// Retrieve the next [`request_pull::Response`].
    pub async fn next(&mut self) -> Option<Result<request_pull::Response, error::RequestPull>> {
        self.replies.recv().await
    }
}

pub(super) struct Runner<'a> {
    tasks: FuturesUnordered<Task<()>>,
    responses: BoxStream<'a, Result<request_pull::Response, error::RequestPull>>,
    replicate: BoxStream<'a, Result<Task<()>, error::Incoming>>,
    done: Option<request_pull::Response>,
}

impl<'a> Runner<'a> {
    pub async fn new<E>(
        spawner: Arc<Spawner>,
        endpoint: E,
        urn: Urn,
        peer: PeerId,
        addrs: Vec<SocketAddr>,
        paths: Arc<Paths>,
    ) -> Result<Runner<'a>, error::RequestPull>
    where
        E: ConnectPeer + Clone + Send + Sync + 'static,
    {
        let ingress = endpoint
            .connect(peer, addrs)
            .await
            .ok_or(error::NoConnection(peer))?;
        let conn = ingress.connection();
        let responses = protocol::io::send::multi_response(
            conn,
            protocol::request_pull::Request { urn },
            protocol::request_pull::FRAMED_BUFSIZ,
        )
        .await?
        .map(|i| i.map_err(error::RequestPull::from))
        .boxed();

        let replicate = match ingress {
            quic::Ingress::Remote(_) => futures::stream::empty().boxed(),
            quic::Ingress::Local { streams, .. } => {
                streams::git(spawner.clone(), paths.clone(), streams).boxed()
            },
        };

        Ok(Self {
            responses,
            replicate,
            tasks: FuturesUnordered::new(),
            done: None,
        })
    }
}

impl<'a> Stream for Runner<'a> {
    type Item = Result<request_pull::Response, error::RequestPull>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done.is_some() {
            while let Poll::Ready(Some(ready)) = self.tasks.poll_next_unpin(cx) {
                if let Err(e) = ready {
                    match e {
                        JoinError::Cancelled => {
                            tracing::warn!("request-pull replication task cancelled");
                            return Poll::Ready(Some(Err(error::RequestPull::Cancelled)));
                        },
                        JoinError::Panicked(e) => {
                            tracing::warn!("request-pull replication task panicked");
                            panic::resume_unwind(e)
                        },
                    }
                }
            }
            if self.tasks.is_empty() {
                return Poll::Ready(Some(Ok(self.done.as_ref().unwrap().clone())));
            } else {
                return Poll::Pending;
            }
        }

        while let Poll::Ready(Some(next_task)) = self.replicate.poll_next_unpin(cx) {
            match next_task {
                Ok(task) => self.tasks.push(task),
                Err(err) => return Poll::Ready(Some(Err(err.into()))),
            }
        }

        while let Poll::Ready(Some(ready)) = self.tasks.poll_next_unpin(cx) {
            if let Err(e) = ready {
                match e {
                    JoinError::Cancelled => {
                        tracing::warn!("request-pull replication task cancelled");
                        return Poll::Ready(Some(Err(error::RequestPull::Cancelled)));
                    },
                    JoinError::Panicked(e) => {
                        tracing::warn!("request-pull replication task panicked");
                        panic::resume_unwind(e)
                    },
                }
            }
        }

        if let Poll::Ready(Some(response)) = self.responses.poll_next_unpin(cx) {
            match response {
                Ok(r) => match r {
                    request_pull::Response::Success(_) | request_pull::Response::Error(_) => {
                        self.done = Some(r.clone());
                        if self.tasks.is_empty() {
                            Poll::Ready(Some(Ok(r)))
                        } else {
                            Poll::Pending
                        }
                    },
                    request_pull::Response::Progress(_) => Poll::Ready(Some(Ok(r))),
                },

                Err(e) => Poll::Ready(Some(Err(e))),
            }
        } else {
            Poll::Pending
        }
    }
}
