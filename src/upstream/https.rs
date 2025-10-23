use std::time::Duration;

use reqwest::header::HeaderValue;
use reqwest::{Body, Client, ClientBuilder, Method, Request, Url};
use thiserror::Error;
use url::Host;

use crate::proto::{OpCode, Packet, Qr, Question, ResourceRecord, ResponseCode};

use super::ResolverError;

#[derive(Clone, Debug, Error)]
pub enum CreateHttpsResolverError {
    #[error("invalid url: {0}")]
    InvalidUrl(url::ParseError),
    #[error("url is missing host section")]
    MissingHost,
    #[error("url scheme is not https")]
    NoHttps,
}

#[derive(Debug)]
pub struct HttpsResolver {
    client: Client,
    pub url: Url,
    pub timeout: Duration,
    pub host: HeaderValue,
}

impl HttpsResolver {
    pub fn new(
        url: &str,
        host: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, CreateHttpsResolverError> {
        let client = ClientBuilder::new().use_rustls_tls().build().unwrap();

        let url: Url = url.parse().map_err(CreateHttpsResolverError::InvalidUrl)?;

        if url.scheme() != "https" {
            return Err(CreateHttpsResolverError::NoHttps);
        }

        let url_host = url.host().ok_or(CreateHttpsResolverError::MissingHost)?;

        if matches!(url_host, Host::Domain(_)) {
            tracing::warn!(
                "the https upstream address is a domain, not a socket address; the domain will be resolved using the system resolver. If the system is set to resolve using this server this will result in a feedback loop and never resolve."
            );
        }

        let host = match host {
            Some(host) => HeaderValue::from_str(&host).unwrap(),
            None => HeaderValue::from_str(&url_host.to_string()).unwrap(),
        };

        Ok(Self {
            client,
            url,
            timeout,
            host,
        })
    }

    pub async fn resolve(&self, question: &Question) -> Result<Vec<ResourceRecord>, ResolverError> {
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
        req.headers_mut().insert("host", self.host.clone());

        *req.body_mut() = Some(Body::from(buf));

        let resp = self
            .client
            .execute(req)
            .await
            .map_err(ResolverError::Http)?;

        if resp.status().is_success() {}

        let data = resp.bytes().await.map_err(ResolverError::Http)?;

        let resp = Packet::decode(&data).map_err(ResolverError::Decode)?;

        match resp.response_code {
            ResponseCode::Ok => Ok(resp.answers),
            ResponseCode::NameError => Err(ResolverError::NonExistantDomain),
            _ => Err(ResolverError::NoAnswer),
        }
    }
}
