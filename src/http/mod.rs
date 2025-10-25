use std::convert::Infallible;
use std::fmt::Write;

use bytes::Bytes;
use futures::future::BoxFuture;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1::Builder;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::tokio::TokioIo;
use tokio::net::TcpListener;

use crate::config;
use crate::state::State;

pub async fn run(http: config::Metrics, state: &'static State) {
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
        (
            "dns_requests_total{protocol=\"udp\"}",
            state.metrics.requests_total_udp.get(),
        ),
        (
            "dns_requests_total{protocol=\"tcp\"}",
            state.metrics.requests_total_tcp.get(),
        ),
        (
            "dns_cache_hits{status=\"noerror\"}",
            state.metrics.cache_hits_noerror.get(),
        ),
        (
            "dns_cache_misses{status=\"noerror\"}",
            state.metrics.cache_misses_noerror.get(),
        ),
        (
            "dns_cache_hits{status=\"nodata\"}",
            state.metrics.cache_hits_nodata.get(),
        ),
        (
            "dns_cache_misses{status=\"nodata\"}",
            state.metrics.cache_misses_nodata.get(),
        ),
        (
            "dns_cache_hits{status=\"nxdomain\"}",
            state.metrics.cache_hits_nxdomain.get(),
        ),
        (
            "dns_cache_misses{status=\"nxdomain\"}",
            state.metrics.cache_misses_nxdomain.get(),
        ),
        ("dns_cache_size", state.metrics.cache_size.get()),
    ] {
        writeln!(body, "{} {}", key, val).unwrap();
    }

    {
        let buckets = state.metrics.resolve_time.buckets.read();
        for (bucket, counter) in &*buckets {
            let nanos = 2_u128.pow(*bucket);

            writeln!(body, "resolve_time{{ns=\"{}\"}} {}", nanos, counter.get()).unwrap();
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}
