mod cache;
mod config;
mod frontend;
mod http;
mod metrics;
mod proto;
mod state;
mod upstream;

use std::path::PathBuf;
use std::process::ExitCode;

use crate::frontend::tcp::TcpServer;
use crate::frontend::udp::UdpServer;
use clap::Parser;
use config::Config;
use hashbrown::HashMap;
use state::State;

#[derive(Clone, Debug, Parser)]
struct Args {
    /// Path to the config file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> ExitCode {
    pretty_env_logger::init();

    let args = Args::parse();

    let config = match Config::from_file(&args.config) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!(
                "failed to read config from {}: {}",
                args.config.to_string_lossy(),
                err,
            );
            return ExitCode::FAILURE;
        }
    };

    let mut zones = HashMap::new();
    for zone in config.zones.values() {
        if zones
            .insert(zone.zone.clone(), zone.upstreams.clone())
            .is_some()
        {
            tracing::error!("zone {} is defined multiple times", &zone.zone);
            return ExitCode::FAILURE;
        }
    }

    let http = config.metrics.clone();
    let state = State::new(zones);
    let state: &'static State = Box::leak(Box::new(state));

    let mut handles = Vec::new();
    if config.frontend.udp.enable {
        let addr = config.frontend.udp.bind;
        handles.push(tokio::task::spawn(async move {
            let server = UdpServer::new(addr).await;
            if let Err(err) = server.poll(&state).await {
                tracing::error!("failed to server DNS server: {}", err)
            }
        }));
    }

    if config.frontend.tcp.enable {
        let addr = config.frontend.tcp.bind;
        handles.push(tokio::task::spawn(async move {
            let server = TcpServer::new(addr).await;
            if let Err(err) = server.poll(&state).await {
                tracing::error!("failed to server DNS server: {}", err)
            }
        }));
    }

    handles.push(tokio::task::spawn(async move {
        state.cleanup().await;
    }));

    if http.enable {
        handles.push(tokio::task::spawn(async move {
            http::run(http, state).await;
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    ExitCode::SUCCESS
}
