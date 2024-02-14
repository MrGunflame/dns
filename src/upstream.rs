use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::{DecodeError, OpCode, Packet, Qr, Question, ResourceRecord, ResponseCode};

pub struct Resolvers {
    pub resolvers: Vec<UpstreamResolver>,
}

#[derive(Debug)]
pub enum ResolverError {
    Io(io::Error),
    Timeout,
    Decode(DecodeError),
    NoAnswer,
}

impl Resolvers {
    pub async fn resolve(&self, question: &Question) -> Result<ResourceRecord, ResolverError> {
        let resolver = self.resolvers.first().unwrap();
        let addr = *resolver.addrs.first().unwrap();

        let local_addr = match addr {
            SocketAddr::V4(_) => SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
            SocketAddr::V6(_) => SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0)),
        };

        let socket = UdpSocket::bind(local_addr)
            .await
            .map_err(ResolverError::Io)?;
        socket.connect(addr).await.map_err(ResolverError::Io)?;

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
        let len = tokio::select! {
            len = socket.recv(&mut buf) => {
                let len = len.map_err(ResolverError::Io)?;
                len
            }
            _ = tokio::time::sleep(resolver.timeout) => return Err(ResolverError::Timeout),
        };

        buf.truncate(len);

        let packet = Packet::decode(&buf[..]).map_err(ResolverError::Decode)?;

        for answer in packet.answers {
            if answer.name == question.name
                && answer.r#type == question.qtype
                && answer.class == question.qclass
            {
                return Ok(answer);
            }
        }

        Err(ResolverError::NoAnswer)
    }
}

pub struct UpstreamResolver {
    pub addrs: Vec<SocketAddr>,
    pub timeout: Duration,
}
