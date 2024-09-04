mod cache;
mod config;
mod frontend;
mod http;
mod metrics;
mod proto;
mod state;
mod upstream;

use crate::frontend::udp::UdpServer;
use config::Config;
use state::State;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let config = Config::from_file("./config.json");

    let state = State::new(config);
    let state: &'static State = Box::leak(Box::new(state));

    let mut handles = Vec::new();
    handles.push(tokio::task::spawn(async move {
        let server = UdpServer::new().await;
        server.poll(&state).await;
    }));
    handles.push(tokio::task::spawn(async move {
        state.cleanup().await;
    }));
    handles.push(tokio::task::spawn(async move {
        http::run(state).await;
    }));

    for handle in handles {
        let _ = handle.await;
    }
}
