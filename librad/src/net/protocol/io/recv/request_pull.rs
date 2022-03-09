// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Implementation of [RFC 702][rfc].
//!
//! [rfc]: https://github.com/radicle-dev/radicle-link/blob/master/docs%2Frfc%2F0702-request-pull.adoc

use std::net::SocketAddr;

use futures::{
    future,
    io::{AsyncWrite, AsyncWriteExt as _, IntoSink},
    SinkExt as _,
    StreamExt as _,
};
use futures_codec::FramedRead;
use thiserror::Error;

use crate::{
    git::{storage, Urn},
    net::{
        connection::{Duplex, RemotePeer as _},
        peer::event::downstream::Gossip,
        protocol::{self, broadcast, control, gossip, io::codec, request_pull, State},
        quic,
        upgrade::{self, Upgraded},
    },
    PeerId,
};

#[derive(Debug, Error)]
enum Error {
    #[error(transparent)]
    Cbor(#[from] minicbor::encode::Error<std::io::Error>),
}

pub(in crate::net::protocol) async fn request_pull<S>(
    state: State<S>,
    stream: Upgraded<upgrade::RequestPull, quic::BidiStream>,
) where
    S: broadcast::LocalStorage<SocketAddr>
        + protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload>
        + storage::Pooled<storage::Storage>
        + Send
        + Sync
        + 'static,
{
    let remote_peer = stream.remote_peer_id();

    #[cfg(not(feature = "replication-v3"))]
    let some_repl = {
        use crate::net::connection::RemoteAddr as _;
        request_pull::SomeReplicate::V2(request_pull::Replicate {
            conn: request_pull::PeerAddr {
                peer: remote_peer,
                addr: stream.remote_addr(),
            },
        })
    };

    #[cfg(feature = "replication-v3")]
    let some_repl = {
        request_pull::SomeReplicate::V3(request_pull::Replicate {
            conn: request_pull::Connection(stream.connection().clone()),
        })
    };

    let (recv, send) = stream.into_stream().split();
    let mut sink = send.into_sink();

    let mut recv = FramedRead::new(recv, codec::Codec::<request_pull::Request>::new());
    if let Some(x) = recv.next().await {
        match x {
            // TODO(finto): seems like it would be useful to report back an error here
            Err(e) => {
                tracing::warn!(err = ?e, "request-pull recv error");
                if let Ok(resp) = encode(
                    &request_pull::Error {
                        message: format!("failed to decode request from `{}`", remote_peer),
                    }
                    .into(),
                ) {
                    sink.send(resp).await.ok();
                }
            },
            Ok(req) => {
                let resp = encode(
                    &handle_request(
                        state,
                        remote_peer,
                        req,
                        some_repl,
                        &mut Reporter { sink: &mut sink },
                    )
                    .await,
                )
                .unwrap_or_else(|e| {
                    tracing::error!(err = ?e, "error handling request");
                    match e {
                        Error::Cbor(_) => encode(
                            &request_pull::Error {
                                message: "internal error".into(),
                            }
                            .into(),
                        )
                        .unwrap(),
                    }
                });

                if let Err(e) = sink.send(resp).await {
                    tracing::warn!(err = ?e, "request-pull send error")
                }
            },
        }
    }
}

struct Reporter<'a, T: Duplex> {
    sink: &'a mut IntoSink<T::Write, Vec<u8>>,
}

impl<'a, T> Reporter<'a, T>
where
    T: Duplex,
    T::Write: AsyncWrite + Unpin,
{
    async fn progress(&mut self, progress: request_pull::Progress) {
        match encode(&progress.into()) {
            Err(e) => tracing::warn!(err = ?e, "request-pull progress error"),
            Ok(progress) => {
                if let Err(e) = self.sink.send(progress).await {
                    tracing::warn!(err = ?e, "request-pull send error")
                }
            },
        }
    }
}

async fn handle_request<'a, S>(
    state: State<S>,
    peer: PeerId,
    request_pull::Request { urn }: request_pull::Request,
    replicate: request_pull::SomeReplicate,
    report: &mut Reporter<'a, quic::BidiStream>,
) -> request_pull::Response
where
    S: broadcast::LocalStorage<SocketAddr>
        + protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload>
        + storage::Pooled<storage::Storage>
        + Send
        + Sync
        + 'static,
{
    report
        .progress(request_pull::progress::authorizing(&peer, &urn))
        .await;
    if !state.request_pull.is_authorized(&peer, &urn) {
        return request_pull::Error {
            message: format!("Unauthorized request-pull from {} for {}", peer, urn),
        }
        .into();
    }

    #[cfg(feature = "replication-v3")]
    let request_pull::SomeReplicate::V3(repl) = replicate;
    #[cfg(not(feature = "replication-v3"))]
    let request_pull::SomeReplicate::V2(repl) = replicate;

    report
        .progress(request_pull::progress::replicating(&peer, &urn))
        .await;
    match state
        .request_pull
        .replicate(&state.spawner, urn.clone(), repl)
        .await
    {
        Ok(refs) => {
            let tips = refs
                .iter()
                .map(|request_pull::Ref { oid, .. }| oid)
                .copied();
            gossip(&state, peer, &urn, tips).await;
            request_pull::Success { refs }.into()
        },
        Err(err) => request_pull::Error {
            message: format!("request-pull replication error: {}", err),
        }
        .into(),
    }
}

async fn gossip<S>(
    state: &State<S>,
    exclude: PeerId,
    urn: &Urn,
    revs: impl Iterator<Item = git_ext::Oid>,
) where
    S: broadcast::LocalStorage<SocketAddr>
        + protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload>
        + Send
        + Sync
        + 'static,
{
    future::join_all(revs.map(|rev| {
        control::gossip(
            state,
            Gossip::Announce(gossip::Payload {
                urn: urn.clone(),
                rev: Some(rev.into()),
                origin: None,
            }),
            Some(exclude),
        )
    }))
    .await;
}

fn encode(resp: &request_pull::Response) -> Result<Vec<u8>, Error> {
    Ok(minicbor::to_vec(resp)?)
}
