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
    git::Urn,
    net::{
        connection::{Duplex, RemotePeer as _},
        peer::event::downstream::Gossip,
        protocol::{
            self,
            control,
            gossip,
            io::codec,
            request_pull::{error, progress, Progress, Ref, Request, Response, Success},
            State,
        },
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

pub(in crate::net::protocol) async fn request_pull<S, A>(
    state: State<S, A>,
    stream: Upgraded<upgrade::RequestPull, quic::BidiStream>,
) where
    S: protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload> + 'static,
    A: protocol::RequestPullGuard,
{
    let remote_peer = stream.remote_peer_id();
    let conn = stream.connection().clone();
    let (recv, send) = stream.into_stream().split();
    let mut sink = send.into_sink();

    let mut recv = FramedRead::new(recv, codec::Codec::<Request>::new());
    if let Some(x) = recv.next().await {
        match x {
            Err(e) => {
                tracing::warn!(err = ?e, "request-pull recv error");
                if let Ok(resp) = encode(&error::decode_failed().into()) {
                    sink.send(resp).await.ok();
                }
            },
            Ok(req) => {
                let resp = encode(
                    &handle_request(
                        state,
                        remote_peer,
                        req,
                        conn,
                        &mut Reporter { sink: &mut sink },
                    )
                    .await,
                )
                .unwrap_or_else(|e| {
                    tracing::error!(err = ?e, "error handling request");
                    match e {
                        Error::Cbor(_) => encode(&error::internal_error().into()).unwrap(),
                    }
                });

                if let Err(e) = sink.send(resp).await {
                    tracing::warn!(err = ?e, "request-pull send error")
                }
            },
        }
    }
}

// Since async closures are unstable, this struct acts as a mechanism
// for allowing progress messages to be sent to a sink.
struct Reporter<'a, T: Duplex> {
    sink: &'a mut IntoSink<T::Write, Vec<u8>>,
}

impl<'a, T> Reporter<'a, T>
where
    T: Duplex,
    T::Write: AsyncWrite + Unpin,
{
    async fn progress(&mut self, progress: Progress) {
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

async fn handle_request<'a, S, A>(
    state: State<S, A>,
    peer: PeerId,
    Request { urn }: Request,
    conn: quic::Connection,
    report: &mut Reporter<'a, quic::BidiStream>,
) -> Response
where
    S: protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload> + 'static,
    A: protocol::RequestPullGuard,
{
    report.progress(progress::authorizing(&urn)).await;
    match state.request_pull.guard(&peer, &urn) {
        Ok(guard) => report.progress(progress::guard(guard)).await,
        Err(err) => return error::guard(err).into(),
    }

    report.progress(progress::replicating(&urn)).await;
    match state
        .request_pull
        .replicate(&state.spawner, urn.clone(), conn)
        .await
    {
        Ok(refs) => {
            let tips = refs.iter().map(|Ref { oid, .. }| oid).copied();
            gossip(&state, peer, &urn, tips).await;
            Success { refs }.into()
        },
        Err(err) => error::replication_error(err).into(),
    }
}

async fn gossip<S, A>(
    state: &State<S, A>,
    exclude: PeerId,
    urn: &Urn,
    revs: impl Iterator<Item = git_ext::Oid>,
) where
    S: protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload> + 'static,
    A: protocol::RequestPullGuard,
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

fn encode(resp: &Response) -> Result<Vec<u8>, Error> {
    Ok(minicbor::to_vec(resp)?)
}
