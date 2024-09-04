use std::time::Duration;

use reqwest::header::HeaderValue;
use reqwest::{Body, Client, ClientBuilder, Method, Request, Url};

use crate::proto::{OpCode, Packet, Qr, Question, ResourceRecord, ResponseCode};

use super::ResolverError;

#[derive(Debug)]
pub struct HttpsResolver {
    client: Client,
    url: Url,
    pub timeout: Duration,
}

impl HttpsResolver {
    pub fn new(url: Url, timeout: Duration) -> Self {
        let client = ClientBuilder::new().use_rustls_tls().build().unwrap();

        Self {
            client,
            url,
            timeout,
        }
    }

    pub async fn resolve(&self, question: &Question) -> Result<ResourceRecord, ResolverError> {
        let packet = Packet {
            transaction_id: rand::random(),
            qr: Qr::Request,
            opcode: OpCode::Query,
            authoritative_answer: false,
            truncated: false,
            recursion_available: false,
            recursion_desired: true,
            response_code: ResponseCode::Ok,
            questions: vec![question.clone()],
            additional: vec![],
            answers: vec![],
            authority: vec![],
        };

        let mut buf = Vec::new();
        packet.encode(&mut buf);

        let mut req = Request::new(Method::POST, self.url.clone());
        req.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("application/dns-message"),
        );

        *req.body_mut() = Some(Body::from(buf));

        let resp = self
            .client
            .execute(req)
            .await
            .map_err(ResolverError::Http)?;

        if resp.status().is_success() {}

        let data = resp.bytes().await.map_err(ResolverError::Http)?;

        let resp = Packet::decode(data).map_err(ResolverError::Decode)?;

        for answer in resp.answers {
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
