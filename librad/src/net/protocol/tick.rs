// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::net::SocketAddr;

use futures::{
    future::{BoxFuture, FutureExt as _, TryFutureExt as _},
    stream::{FuturesOrdered, StreamExt as _},
};

use super::{error, gossip, graft, io, membership, PeerInfo, ProtocolStorage, State};
use crate::PeerId;

#[derive(Debug)]
pub(super) enum Tock<A, P> {
    /// Send to connected peer, or notify of connection loss
    SendConnected { to: PeerId, message: io::Rpc<A, P> },

    /// Attempt to connect + send, ignore failure
    AttemptSend {
        to: PeerInfo<A>,
        message: io::Rpc<A, P>,
    },

    /// Close connections due to eviction from partial view
    Disconnect { peer: PeerId },
}

#[tracing::instrument(level = "debug", skip(state))]
pub(super) async fn tock<S>(state: State<S>, tock: Tock<SocketAddr, gossip::Payload>)
where
    S: ProtocolStorage<SocketAddr, Update = gossip::Payload> + Clone + 'static,
{
    let mut mcfly = FuturesOrdered::new();
    mcfly.push(one_tock(state.clone(), tock));

    while let Some(res) = mcfly.next().await {
        tracing::debug!("tock");
        if let Err(e) = res {
            match e {
                error::Tock::Reliable(error::ReliableSend { cont, source }) => {
                    tracing::warn!(err = ?source, "reliable send error");
                    let info = || io::peer_advertisement(&state.endpoint);
                    for tick in cont {
                        mcfly.extend(
                            membership::collect_tocks(&state.membership, &info, tick)
                                .into_iter()
                                .map(|tock| one_tock(state.clone(), tock)),
                        )
                    }
                },
                error::Tock::Unreliable(source) => {
                    tracing::warn!(err = ?source, "unreliable send error");
                },
            }
        }
    }
}

fn one_tock<S>(
    mut state: State<S>,
    tock: Tock<SocketAddr, gossip::Payload>,
) -> BoxFuture<'static, Result<(), error::Tock<SocketAddr>>>
where
    S: ProtocolStorage<SocketAddr, Update = gossip::Payload> + Clone + 'static,
{
    use Tock::*;

    async move {
        match tock {
            SendConnected { to, message } => match state.endpoint.get_connection(to) {
                None => {
                    let membership::TnT { trans, ticks: cont } =
                        state.membership.connection_lost(to);
                    trans.into_iter().for_each(|evt| state.phone.emit(evt));

                    Err(error::Tock::Reliable(error::ReliableSend {
                        cont,
                        source: error::ReliableSendSource::NotConnected { to },
                    }))
                },

                Some(conn) => {
                    io::send_rpc(&conn, message)
                        .map_err(|e| {
                            let membership::TnT { trans, ticks: cont } =
                                state.membership.connection_lost(to);
                            trans.into_iter().for_each(|evt| state.phone.emit(evt));

                            error::Tock::Reliable(error::ReliableSend {
                                cont,
                                source: e.into(),
                            })
                        })
                        .await
                },
            },

            AttemptSend { to, message } => {
                let conn = match state.endpoint.get_connection(to.peer_id) {
                    Some(conn) => conn,
                    None => {
                        let conn =
                            io::connections::connect_accept(state.clone(), to.peer_id, to.clone())
                                .await
                                .ok_or(error::BestEffortSend::CouldNotConnect { to })?;
                        state.graft_trigger(conn.clone(), graft::Source::Outgoing);
                        conn
                    },
                };
                Ok(io::send_rpc(&conn, message)
                    .await
                    .map_err(error::BestEffortSend::SendGossip)?)
            },

            Disconnect { peer } => {
                state.endpoint.disconnect(&peer);
                Ok(())
            },
        }
    }
    .boxed()
}
