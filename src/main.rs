mod cache;
mod config;
mod frontend;
mod http;
mod metrics;
mod proto;
mod state;
mod upstream;

use std::path::PathBuf;

use crate::frontend::tcp::TcpServer;
use crate::frontend::udp::UdpServer;
use clap::Parser;
use config::Config;
use state::State;

#[derive(Clone, Debug, Parser)]
struct Args {
    /// Path to the config file.
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let args = Args::parse();

    let config = Config::from_file(&args.config);

    let addr = config.bind;
    let http = config.http.clone();
    let state = State::new(config);
    let state: &'static State = Box::leak(Box::new(state));

    let mut handles = Vec::new();
    handles.push(tokio::task::spawn(async move {
        let server = UdpServer::new(addr).await;
        if let Err(err) = server.poll(&state).await {
            tracing::error!("failed to server DNS server: {}", err)
        }
    }));
    handles.push(tokio::task::spawn(async move {
        let server = TcpServer::new(addr).await;
        if let Err(err) = server.poll(&state).await {
            tracing::error!("failed to server DNS server: {}", err)
        }
    }));
    handles.push(tokio::task::spawn(async move {
        state.cleanup().await;
    }));

    if http.enabled {
        handles.push(tokio::task::spawn(async move {
            http::run(http, state).await;
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }
}
