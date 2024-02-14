use std::io;
use std::net::SocketAddr;
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

        let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        socket.connect(addr).await.unwrap();

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

        socket.send(&buf).await.unwrap();

        let mut buf = vec![0; 1500];
        let len = tokio::select! {
            len = socket.recv(&mut buf) => {
                let len = len.unwrap();
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
