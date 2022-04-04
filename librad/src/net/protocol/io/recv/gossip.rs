// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{iter, net::SocketAddr};

use futures::{
    io::{AsyncRead, BufReader},
    stream::StreamExt as _,
};
use futures_codec::FramedRead;

use crate::{
    net::{
        connection::RemotePeer,
        protocol::{
            self,
            broadcast,
            gossip,
            info::PeerInfo,
            io::{codec, peer_advertisement},
            membership,
            state,
            ProtocolStorage,
            RequestPullGuard,
            State,
            TinCans,
        },
        upgrade::{self, Upgraded},
    },
    PeerId,
};

pub(in crate::net::protocol) struct GossipState {
    pub local_id: PeerId,
    pub endpoint: protocol::Endpoint,
    pub membership: membership::Hpv<protocol::Pcg64Mcg, SocketAddr>,
    pub phone: TinCans,
}

impl<S, G> From<state::State<S, G>> for GossipState {
    fn from(_: state::State<S, G>) -> Self {
        todo!()
    }
}

pub(in crate::net::protocol) async fn gossip<S, G, T>(
    gossip: GossipState,
    state: State<S, G>,
    stream: Upgraded<upgrade::Gossip, T>,
) where
    S: ProtocolStorage<SocketAddr, Update = gossip::Payload> + Clone + 'static,
    G: RequestPullGuard,
    T: RemotePeer + AsyncRead + Unpin,
{
    let remote_id = stream.remote_peer_id();

    let mut recv = FramedRead::new(
        BufReader::with_capacity(100, stream.into_stream()),
        codec::Gossip::new(),
    );

    while let Some(x) = recv.next().await {
        match x {
            Err(e) => {
                tracing::warn!(err = ?e, "gossip recv error");
                let membership::TnT { trans, ticks } = gossip.membership.connection_lost(remote_id);
                state::emit(&gossip.phone, trans);
                state
                    .tick(membership::tocks(
                        &gossip.membership,
                        peer_advertisement(&gossip.endpoint),
                        ticks,
                    ))
                    .await;

                break;
            },

            Ok(msg) => {
                let peer_info = || PeerInfo {
                    peer_id: gossip.local_id,
                    advertised_info: peer_advertisement(&gossip.endpoint)(),
                    seen_addrs: iter::empty().into(),
                };
                match state
                    .gossip
                    .apply(&gossip.membership, peer_info, remote_id, msg)
                    .await
                {
                    // Partial view states diverge apparently, and the stream is
                    // (assumed to be) unidirectional. Thus, send a DISCONNECT
                    // to sync states.
                    Err(broadcast::Error::Unsolicited { remote_id, .. }) => {
                        tracing::warn!(
                            remote_id = %remote_id,
                            "unsolicited broadcast message, sending disconnect"
                        );
                        state
                            .tick(membership::tocks(
                                &gossip.membership,
                                peer_advertisement(&gossip.endpoint),
                                Some(disconnect(remote_id)),
                            ))
                            .await;

                        break;
                    },

                    Ok((may_event, tocks)) => {
                        state::emit(&gossip.phone, may_event);
                        state.tick(tocks).await;
                    },
                }
            },
        }
    }
}

fn disconnect<A>(remote_id: PeerId) -> membership::Tick<A> {
    membership::Tick::Reply {
        to: remote_id,
        message: membership::Message::Disconnect,
    }
}
