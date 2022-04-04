// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Implementation of [RFC 702][rfc].
//!
//! [rfc]: https://github.com/radicle-dev/radicle-link/blob/master/docs%2Frfc%2F0702-request-pull.adoc

use std::{net::SocketAddr, sync::Arc};

use futures::{
    io::{AsyncWrite, AsyncWriteExt as _, BufReader, BufWriter, IntoSink},
    SinkExt as _,
    StreamExt as _,
};
use futures_codec::FramedRead;
use link_async::Spawner;
use thiserror::Error;

use crate::{
    net::{
        connection::{Duplex, RemotePeer as _},
        protocol::{
            self,
            gossip,
            io::codec,
            request_pull::{self, error, progress, Progress, Request, Response},
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

pub(in crate::net::protocol) async fn request_pull<S, G>(
    spawner: Arc<Spawner>,
    state: request_pull::State<S, G>,
    stream: Upgraded<upgrade::RequestPull, quic::BidiStream>,
) where
    S: protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload> + 'static,
    G: protocol::RequestPullGuard,
{
    let remote_peer = stream.remote_peer_id();
    let conn = stream.connection().clone();
    let (recv, send) = stream.into_stream().split();
    let recv = BufReader::with_capacity(request_pull::FRAMED_BUFSIZ, recv);
    let send = BufWriter::with_capacity(request_pull::FRAMED_BUFSIZ, send);
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
                        spawner,
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
struct Reporter<'a, W> {
    sink: &'a mut IntoSink<W, Vec<u8>>,
}

impl<'a, W> Reporter<'a, W>
where
    W: AsyncWrite + Unpin,
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

async fn handle_request<'a, S, G, W>(
    spawner: Arc<Spawner>,
    state: request_pull::State<S, G>,
    peer: PeerId,
    Request { urn }: Request,
    conn: quic::Connection,
    report: &mut Reporter<'a, W>,
) -> Response
where
    S: protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload> + 'static,
    G: protocol::RequestPullGuard,
    W: AsyncWrite + Unpin,
{
    report.progress(progress::authorizing(&urn)).await;
    match state.guard(&peer, &urn) {
        Ok(guard) => report.progress(progress::guard(guard)).await,
        Err(err) => return error::guard(err).into(),
    }

    report.progress(progress::replicating(&urn)).await;
    match state.replicate(&spawner, urn.clone(), conn).await {
        Ok(success) => success.into(),
        Err(err) => error::replication_error(err).into(),
    }
}

// TODO(finto): this needs to go elsewhere
// async fn gossip<S, G>(
//     state: &State<S, G>,
//     exclude: PeerId,
//     urn: &Urn,
//     revs: impl Iterator<Item = git_ext::Oid>,
// ) where
//     S: protocol::ProtocolStorage<SocketAddr, Update = gossip::Payload> +
// 'static,     G: protocol::RequestPullGuard,
// {
//     future::join_all(revs.map(|rev| {
//         control::gossip(
//             state,
//             Gossip::Announce(gossip::Payload {
//                 urn: urn.clone(),
//                 rev: Some(rev.into()),
//                 origin: None,
//             }),
//             Some(exclude),
//         )
//     }))
//     .await;
// }

fn encode(resp: &Response) -> Result<Vec<u8>, Error> {
    Ok(minicbor::to_vec(resp)?)
}
