use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use futures::stream::{FuturesOrdered, FuturesUnordered, StreamExt};
use futures::{FutureExt, select_biased};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::frontend::handle_query;
use crate::proto::{DecodeError, Packet};
use crate::state::State;

const TIMEOUT: Duration = Duration::from_secs(2 * 60);

/// Maximum number of currently progressing pipelined queries.
///
/// The server will stop accepting new queries from the client once this number of queries is
/// reached and only continue once queries resolve.
const MAX_QUEUED_QUERIES: usize = 64;

#[derive(Debug)]
pub struct TcpServer {
    listener: TcpListener,
}

impl TcpServer {
    pub async fn new(addr: SocketAddr) -> Self {
        let listener = TcpListener::bind(addr).await.unwrap();
        Self { listener }
    }

    pub async fn poll(&self, state: &State) -> Result<(), io::Error> {
        let mut tasks = FuturesUnordered::new();

        loop {
            let accept = async {
                let (stream, addr) = self.listener.accept().await?;
                tracing::debug!("accepting TCP connection from {}", addr);
                Ok::<_, io::Error>(stream)
            };

            if tasks.is_empty() {
                match accept.await {
                    Ok(stream) => tasks.push(handle_conn(stream, state)),
                    Err(err) => return Err(err),
                }

                continue;
            }

            select_biased! {
                _ = tasks.next().fuse() => (),
                res = accept.fuse() => match res {
                    Ok(stream) => tasks.push(handle_conn(stream, state)),
                    Err(err) => return Err(err),
                }
            }
        }
    }
}

async fn handle_conn(mut stream: TcpStream, state: &State) {
    async fn handle_conn_inner(stream: &mut TcpStream, state: &State) -> Result<(), StreamError> {
        let mut tasks = FuturesOrdered::new();

        let (mut reader, mut writer) = stream.split();

        let mut write_packet: Option<Vec<u8>> = None;
        loop {
            if tasks.is_empty() && write_packet.is_none() {
                select_biased! {
                    _ = tokio::time::sleep(TIMEOUT.into()).fuse() => return Err(StreamError::Timeout),
                    res = read_query(&mut reader).fuse() => {
                        let packet = res?;
                        tasks.push_back(handle_query(state, packet));
                    }
                }
            }

            if let Some(packet) = &mut write_packet
                && tasks.len() < MAX_QUEUED_QUERIES
            {
                select_biased! {
                    res = write_resp(&mut writer, &packet).fuse() => {
                        res.map_err(StreamError::Io)?;
                        write_packet = None;
                    }
                    res = read_query(&mut reader).fuse() => {
                        let packet = res?;
                        tasks.push_back(handle_query(state, packet));
                        state.metrics.requests_total_tcp.inc();
                    }
                }
            } else {
                select_biased! {
                    resp = tasks.next() => {
                        if let Some(Some(resp)) = resp {
                            write_packet = Some(encode_packet(resp));
                        }
                    }
                }
            }
        }
    }

    if let Err(err) = handle_conn_inner(&mut stream, state).await {
        tracing::debug!("failed to serve tcp connection: {:?}", err);
    }

    if let Err(err) = stream.shutdown().await {
        tracing::debug!("failed to shutdown tcp connection: {}", err);
    }
}

async fn read_query(mut stream: impl AsyncRead + Unpin) -> Result<Packet, StreamError> {
    let len = stream.read_u16().await.map_err(StreamError::Io)?;

    let mut buf = vec![0; len as usize];
    stream.read_exact(&mut buf).await.map_err(StreamError::Io)?;

    let packet = match Packet::decode(&buf) {
        Ok(packet) => packet,
        Err(err) => {
            tracing::debug!("failed to decode packet: {:?}", err);
            return Err(StreamError::Decode(err));
        }
    };

    Ok(packet)
}

fn encode_packet(mut packet: Packet) -> Vec<u8> {
    let mut buf = Vec::new();
    packet.encode(&mut buf);

    let len = match u16::try_from(buf.len()) {
        Ok(len) => len,
        Err(_) => {
            packet.truncated = true;

            buf.clear();
            packet.encode(&mut buf);
            buf.truncate(u16::MAX.into());

            u16::MAX
        }
    };

    buf.resize(len as usize + 2, 0);
    buf.copy_within(..usize::from(len), 2);
    buf[0..2].copy_from_slice(&len.to_be_bytes());

    buf
}

async fn write_resp(mut stream: impl AsyncWrite + Unpin, buf: &[u8]) -> Result<(), io::Error> {
    stream.write_all(&buf).await?;
    Ok(())
}

#[derive(Debug)]
enum StreamError {
    Io(io::Error),
    Decode(DecodeError),
    Timeout,
}
