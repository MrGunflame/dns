use std::net::SocketAddr;

use tokio::net::UdpSocket;

use crate::proto::{OpCode, Packet, Qr, ResourceRecord, ResponseCode};
use crate::state::State;

#[derive(Debug)]
pub struct UdpServer {
    socket: UdpSocket,
}

impl UdpServer {
    pub async fn new() -> Self {
        let socket = UdpSocket::bind("[::]:5353").await.unwrap();
        Self { socket }
    }

    pub async fn poll(&self, state: &State) {
        loop {
            let mut buf = vec![0; 1500];

            let (len, addr) = self.socket.recv_from(&mut buf).await.unwrap();
            buf.truncate(len);

            let packet = match Packet::decode(&buf[..]) {
                Ok(packet) => packet,
                Err(err) => {
                    tracing::trace!("failed to decode packet: {:?}", err);
                    continue;
                }
            };

            handle_request(packet, addr, &self.socket, &state).await;
        }
    }
}

async fn handle_request(packet: Packet, addr: SocketAddr, socket: &UdpSocket, state: &State) {
    let mut answers = Vec::new();
    let mut response_code = ResponseCode::Ok;

    for question in &packet.questions {
        match state.resolve(question).await {
            Ok(answer) => {
                answers.push(ResourceRecord {
                    name: question.name.clone(),
                    r#type: answer.r#type,
                    class: answer.class,
                    ttl: answer.ttl().as_secs() as u32,
                    rddata: answer.data.clone(),
                });
            }
            Err(err) => {
                tracing::error!("failed to resolve query: {:?}", err);

                // NOTE: The DNS standard is not clear how to handle
                // multiple questions in a single packet.
                // We attempt to handle all questions, but if any question
                // fails to resolve we return no answers.
                answers.clear();
                response_code = ResponseCode::ServerFailure;
                break;
            }
        };
    }

    let response = Packet {
        transaction_id: packet.transaction_id,
        qr: Qr::Response,
        opcode: OpCode::Query,
        authoritative_answer: false,
        recursion_desired: packet.recursion_desired,
        recursion_available: true,
        truncated: false,
        response_code,
        questions: packet.questions,
        answers,
        additional: Vec::new(),
        authority: Vec::new(),
    };

    let mut buf = Vec::new();
    response.encode(&mut buf);

    if let Err(err) = socket.send_to(&buf, addr).await {
        tracing::debug!("failed to respond to {}: {}", addr, err);
    }
}
