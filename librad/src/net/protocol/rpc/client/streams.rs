// Copyright © 2022 The Radicle Link Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use either::Either;
use futures::{Stream, StreamExt as _};
use link_async::{Spawner, Task};

use crate::{
    net::{
        connection::{CloseReason, RemoteAddr as _, RemotePeer},
        protocol::io,
        quic,
        upgrade,
    },
    paths::Paths,
};

use super::error;

/// Dispatch a bidirectional, git stream.
///
/// This will deny all other streams.
///
/// # Panics
///
/// Panics if one of the tasks spawned by this function panics.
#[tracing::instrument(
    skip(spawner, streams),
    fields(
        remote_id = %streams.remote_peer_id(),
        remote_addr = %streams.remote_addr()
    )
)]
pub(in crate::net::protocol) async fn git<I>(
    spawner: Arc<Spawner>,
    paths: Arc<Paths>,
    streams: quic::IncomingStreams<I>,
) -> Result<Task<()>, error::Incoming>
where
    I: Stream<Item = quic::Result<Either<quic::BidiStream, quic::RecvStream>>> + Unpin,
{
    use Either::{Left, Right};

    let streams = streams.fuse();
    futures::pin_mut!(streams);
    match streams.next().await {
        None => {
            tracing::info!("connection lost");
            Err(error::Incoming::ConnectionLost)
        },
        Some(stream) => {
            tracing::info!("new ingress stream");
            match stream {
                Ok(s) => match s {
                    Left(bidi) => Ok(spawner.spawn(incoming::bidi(paths.clone(), bidi))),
                    Right(uni) => {
                        incoming::deny_uni(uni);
                        Err(error::Incoming::Uni)
                    },
                },
                Err(e) => {
                    tracing::warn!(err = ?e, "ingress stream error");
                    Err(e.into())
                },
            }
        },
    }
}

mod incoming {
    use super::*;

    pub(super) async fn bidi(paths: Arc<Paths>, stream: quic::BidiStream) {
        use upgrade::SomeUpgraded::*;

        match upgrade::with_upgraded(stream).await {
            Err(upgrade::Error { stream, source }) => {
                tracing::warn!(err = ?source, "invalid upgrade");
                stream.close(CloseReason::InvalidUpgrade)
            },

            Ok(Git(up)) => io::recv::git(&paths, up).await,
            Ok(Gossip(up)) => deny_bidi(up.into_stream(), "gossip"),
            Ok(Membership(up)) => deny_bidi(up.into_stream(), "membership"),
            Ok(Interrogation(up)) => deny_bidi(up.into_stream(), "interrogation"),
            Ok(RequestPull(up)) => deny_bidi(up.into_stream(), "request-pull"),
        }
    }

    pub(super) fn deny_uni(stream: quic::RecvStream) {
        tracing::warn!("unidirectional requested");
        stream.close(CloseReason::InvalidUpgrade)
    }

    fn deny_bidi(stream: quic::BidiStream, kind: &str) {
        tracing::warn!("non-git bidirectional {} requested", kind);
        stream.close(CloseReason::InvalidUpgrade)
    }
}
