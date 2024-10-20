use std::convert::Infallible;
use std::fmt::Write;
use std::sync::atomic::Ordering;

use bytes::Bytes;
use futures::future::BoxFuture;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1::Builder;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::tokio::TokioIo;
use tokio::net::TcpListener;

use crate::config::Http;
use crate::state::State;

pub async fn run(http: Http, state: &'static State) {
    let listener = TcpListener::bind(http.bind).await.unwrap();

    loop {
        let (stream, _) = listener.accept().await.unwrap();

        let conn = Builder::new().serve_connection(TokioIo::new(stream), RootService { state });
        tokio::task::spawn(conn);
    }
}

struct RootService {
    state: &'static State,
}

impl Service<Request<Incoming>> for RootService {
    type Response = Response<Full<Bytes>>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let state = self.state;
        Box::pin(async move {
            let resp = match req.uri().path() {
                "/metrics" => metrics(state).await,
                _ => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Full::new(Bytes::new()))
                    .unwrap(),
            };

            Ok(resp)
        })
    }
}

async fn metrics(state: &State) -> Response<Full<Bytes>> {
    let mut body = String::new();
    for (key, val) in [
        ("dns_cache_hits", &state.metrics.cache_hits),
        ("dns_cache_misses", &state.metrics.cache_misses),
        ("dns_cache_size", &state.metrics.cache_size),
    ] {
        writeln!(body, "{} {}", key, val.load(Ordering::Relaxed)).unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}
