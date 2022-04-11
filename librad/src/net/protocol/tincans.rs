// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{net::SocketAddr, sync::Arc};

use parking_lot::Mutex;
pub use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast as tincan, oneshot::Receiver};

use super::{
    event::{self, Downstream},
    gossip,
};
use crate::{
    net::quic::{self, ConnectPeer},
    PeerId,
};

pub struct Connected(pub(crate) quic::Connection);

#[derive(Clone)]
pub struct TinCans {
    pub(super) downstream: tincan::Sender<event::Downstream>,
    pub(super) upstream: tincan::Sender<event::Upstream>,
}

impl TinCans {
    pub fn new() -> Self {
        Self {
            downstream: tincan::channel(16).0,
            upstream: tincan::channel(16).0,
        }
    }

    pub fn announce(&self, have: gossip::Payload) -> Result<(), gossip::Payload> {
        use event::downstream::Gossip::Announce;

        self.downstream
            .send(Downstream::Gossip(Announce(have)))
            .and(Ok(()))
            .map_err(|tincan::error::SendError(e)| match e {
                Downstream::Gossip(g) => g.payload(),
                _ => unreachable!(),
            })
    }

    pub fn query(&self, want: gossip::Payload) -> Result<(), gossip::Payload> {
        use event::downstream::Gossip::Query;

        self.downstream
            .send(Downstream::Gossip(Query(want)))
            .and(Ok(()))
            .map_err(|tincan::error::SendError(e)| match e {
                Downstream::Gossip(g) => g.payload(),
                _ => unreachable!(),
            })
    }

    pub async fn connected_peers(&self) -> Vec<PeerId> {
        use event::downstream::Info::*;

        let (tx, rx) = replier();
        if let Err(tincan::error::SendError(e)) =
            self.downstream.send(Downstream::Info(ConnectedPeers(tx)))
        {
            match e {
                Downstream::Info(ConnectedPeers(reply)) => {
                    reply
                        .lock()
                        .take()
                        .expect("if chan send failed, there can't be another contender")
                        .send(vec![])
                        .ok();
                },

                _ => unreachable!(),
            }
        }

        rx.await.unwrap_or_default()
    }

    pub async fn membership(&self) -> event::downstream::MembershipInfo {
        use event::downstream::{Info::*, MembershipInfo};

        let (tx, rx) = replier();
        if let Err(tincan::error::SendError(e)) =
            self.downstream.send(Downstream::Info(Membership(tx)))
        {
            match e {
                Downstream::Info(Membership(reply)) => {
                    reply
                        .lock()
                        .take()
                        .expect("if chan send failed, there can't be another contender")
                        .send(MembershipInfo::default())
                        .ok();
                },
                _ => unreachable!(),
            }
        }

        rx.await.unwrap_or_default()
    }

    pub async fn stats(&self) -> event::downstream::Stats {
        use event::downstream::{Info::*, Stats};

        let (tx, rx) = replier();
        if let Err(tincan::error::SendError(e)) = self.downstream.send(Downstream::Info(Stats(tx)))
        {
            match e {
                Downstream::Info(Stats(reply)) => {
                    reply
                        .lock()
                        .take()
                        .expect("if chan send failed, there can't be another contender")
                        .send(Stats::default())
                        .ok();
                },

                _ => unreachable!(),
            }
        }

        rx.await.unwrap_or_default()
    }

    pub async fn connect(&self, peer: impl Into<(PeerId, Vec<SocketAddr>)>) -> Option<Connected> {
        use event::downstream::Connect;

        let (tx, rx) = replier();
        if let Err(tincan::error::SendError(e)) =
            self.downstream.send(Downstream::Connect(Connect {
                peer: peer.into(),
                reply: tx,
            }))
        {
            match e {
                Downstream::Connect(Connect { reply, .. }) => {
                    reply
                        .lock()
                        .take()
                        .expect("if chan send failed, there can't be another contender")
                        .send(None)
                        .ok();
                },

                _ => unreachable!(),
            }
        }

        rx.await.ok().flatten().map(Connected)
    }

    pub fn subscribe(&self) -> impl futures::Stream<Item = Result<event::Upstream, RecvError>> {
        let mut r = self.upstream.subscribe();
        async_stream::stream! { loop { yield r.recv().await } }
    }

    pub(crate) fn emit(&self, evt: impl Into<event::Upstream>) {
        self.upstream.send(evt.into()).ok();
    }
}

impl Default for TinCans {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConnectPeer for TinCans {
    async fn connect<'a, Addrs>(&self, peer: PeerId, addrs: Addrs) -> Option<quic::Ingress<'a>>
    where
        Addrs: IntoIterator<Item = SocketAddr> + Send,
    {
        let addrs = addrs.into_iter().collect();
        Self::connect(self, (peer, addrs))
            .await
            .map(|Connected(c)| quic::Ingress::Remote(c))
    }
}

fn replier<T>() -> (event::downstream::Reply<T>, Receiver<T>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (Arc::new(Mutex::new(Some(tx))), rx)
}
