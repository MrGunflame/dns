use std::time::Instant;

use crate::proto::{OpCode, Packet, Qr, ResourceRecord, ResponseCode};
use crate::state::State;
use crate::upstream::ResolverError;

pub mod tcp;
pub mod udp;

pub async fn handle_query(state: &State, packet: Packet) -> Option<Packet> {
    let mut answers = Vec::new();
    let mut response_code = ResponseCode::Ok;

    // We don't count non-RD queries for metrics because they don't
    // actually require any work.
    if packet.recursion_desired {
        let now = Instant::now();

        for question in &packet.questions {
            match state.resolve(question).await {
                Ok(resp) => {
                    for answer in resp {
                        answers.push(ResourceRecord {
                            r#type: answer.r#type,
                            class: answer.class,
                            ttl: answer.ttl().as_secs() as u32,
                            rdata: answer.data,
                            name: answer.name,
                        });
                    }
                }
                Err(ResolverError::NonExistantDomain) => {
                    response_code = ResponseCode::NameError;
                    break;
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

        state.metrics.resolve_time.insert(now.elapsed());
    }

    Some(Packet {
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
    })
}
