// Copyright © 2019-2020 The Radicle Foundation <hello@radicle.foundation>
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::{borrow::Cow, net::SocketAddr};

use futures::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt as _, BufReader, BufWriter},
    SinkExt as _,
    StreamExt as _,
};
use futures_codec::FramedRead;
use thiserror::Error;

use crate::net::{
    connection::Duplex,
    protocol::{
        cache,
        interrogation::{self, Request, Response},
        io::{self, codec},
        Endpoint,
    },
    upgrade::{self, Upgraded},
};

#[derive(Debug, Error)]
enum Error {
    #[error(transparent)]
    Cbor(#[from] minicbor::encode::Error<std::io::Error>),
}

lazy_static! {
    static ref INTERNAL_ERROR: Vec<u8> =
        encode(&Response::Error(interrogation::Error::Internal)).unwrap();
}

pub(in crate::net::protocol) async fn interrogation<T>(
    endpoint: Endpoint,
    caches: cache::Caches,
    stream: Upgraded<upgrade::Interrogation, T>,
) where
    T: Duplex<Addr = SocketAddr>,
    T::Read: AsyncRead + Unpin,
    T::Write: AsyncWrite + Unpin,
{
    let remote_addr = stream.remote_addr();

    let (recv, send) = stream.into_stream().split();
    let recv = BufReader::with_capacity(interrogation::FRAMED_BUFSIZ, recv);
    let send = BufWriter::with_capacity(interrogation::FRAMED_BUFSIZ, send);

    let mut recv = FramedRead::new(recv, codec::Codec::<interrogation::Request>::new());
    if let Some(x) = recv.next().await {
        match x {
            Err(e) => tracing::warn!(err = ?e, "interrogation recv error"),
            Ok(req) => {
                let resp = handle_request(&endpoint, &caches.urns, remote_addr, req)
                    .map(Cow::from)
                    .unwrap_or_else(|e| {
                        tracing::error!(err = ?e, "error handling request");
                        match e {
                            Error::Cbor(_) => Cow::from(&*INTERNAL_ERROR),
                        }
                    });

                if let Err(e) = send.into_sink().send(resp).await {
                    tracing::warn!(err = ?e, "interrogation send error")
                }
            },
        }
    }
}

fn handle_request(
    endpoint: &Endpoint,
    urns: &cache::urns::Filter,
    remote_addr: SocketAddr,
    req: interrogation::Request,
) -> Result<Vec<u8>, Error> {
    use either::Either::*;

    match req {
        Request::GetAdvertisement => {
            Left(Response::Advertisement(io::peer_advertisement(endpoint)()))
        },
        Request::EchoAddr => Left(Response::YourAddr(remote_addr)),
        Request::GetUrns => {
            let urns = urns.get();
            Right(encode(&Response::<SocketAddr>::Urns(Cow::Borrowed(&*urns))))
        },
    }
    .right_or_else(|resp| encode(&resp))
}

fn encode(resp: &interrogation::Response<SocketAddr>) -> Result<Vec<u8>, Error> {
    Ok(minicbor::to_vec(resp)?)
}
