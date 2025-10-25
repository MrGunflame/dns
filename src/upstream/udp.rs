use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::proto::{OpCode, Packet, Qr, Question, ResponseCode};

use super::ResolverError;

#[derive(Debug)]
pub struct UdpResolver {
    pub addr: SocketAddr,
    pub timeout: Duration,
}

impl UdpResolver {
    pub fn new(addr: SocketAddr, timeout: Duration) -> Self {
        Self { addr, timeout }
    }

    pub async fn resolve(&self, question: &Question) -> Result<Packet, ResolverError> {
        let local_addr = match self.addr {
            SocketAddr::V4(_) => SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
            SocketAddr::V6(_) => SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0)),
        };

        let socket = UdpSocket::bind(local_addr)
            .await
            .map_err(ResolverError::Io)?;
        socket.connect(self.addr).await.map_err(ResolverError::Io)?;

        let packet = Packet {
            transaction_id: rand::random(),
            qr: Qr::Request,
            opcode: OpCode::Query,
            authoritative_answer: false,
            truncated: false,
            recursion_desired: true,
            recursion_available: false,
            response_code: ResponseCode::Ok,
            questions: vec![question.clone()],
            answers: vec![],
            additional: vec![],
            authority: vec![],
        };

        let mut buf = Vec::new();
        packet.encode(&mut buf);

        socket.send(&buf).await.map_err(ResolverError::Io)?;

        let mut buf = vec![0; 1500];
        let len = socket.recv(&mut buf).await.map_err(ResolverError::Io)?;
        buf.truncate(len);

        let packet = Packet::decode(&buf[..]).map_err(ResolverError::Decode)?;

        if packet.truncated {
            return Err(ResolverError::Truncated);
        }

        Ok(packet)
    }
}
