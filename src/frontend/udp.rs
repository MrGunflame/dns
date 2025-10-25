use std::io;
use std::net::SocketAddr;

use futures::stream::{FuturesOrdered, StreamExt};
use futures::{FutureExt, select_biased};
use tokio::net::UdpSocket;

use crate::frontend::handle_query;
use crate::proto::Packet;
use crate::state::State;

#[derive(Debug)]
pub struct UdpServer {
    socket: UdpSocket,
}

impl UdpServer {
    pub async fn new(addr: SocketAddr) -> Self {
        let socket = UdpSocket::bind(addr).await.unwrap();
        Self { socket }
    }

    pub async fn poll(&self, state: &State) -> Result<(), io::Error> {
        let mut tasks = FuturesOrdered::new();

        loop {
            let incoming = async {
                let mut buf = [0; 1500];

                let (len, addr) = self.socket.recv_from(&mut buf).await?;

                let packet = match Packet::decode(&buf[..len]) {
                    Ok(packet) => packet,
                    Err(err) => {
                        tracing::trace!("failed to decode packet: {:?}", err);
                        return Ok(None);
                    }
                };

                Ok(Some(Request { packet, addr }))
            };

            if tasks.is_empty() {
                match incoming.await {
                    Ok(Some(req)) => {
                        tasks.push_back(handle_request(req.packet, req.addr, &self.socket, state));
                    }
                    Ok(None) => (),
                    Err(err) => return Err(err),
                }

                continue;
            }

            select_biased! {
                task = tasks.next().fuse() => {
                    debug_assert!(task.is_some());
                },
                req = incoming.fuse() => match req {
                    Ok(Some(req)) => tasks.push_back(handle_request(req.packet,req.addr, &self.socket, state)),
                    Ok(None) => (),
                    Err(err) => return Err(err),
                }
            }
        }
    }
}

async fn handle_request(packet: Packet, addr: SocketAddr, socket: &UdpSocket, state: &State) {
    state.metrics.requests_total_udp.inc();

    let Some(resp) = handle_query(state, packet).await else {
        return;
    };

    let mut buf = Vec::new();
    resp.encode(&mut buf);

    if let Err(err) = socket.send_to(&buf, addr).await {
        tracing::debug!("failed to respond to {}: {}", addr, err);
    }
}

#[derive(Clone, Debug)]
struct Request {
    packet: Packet,
    addr: SocketAddr,
}
