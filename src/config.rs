use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub bind: SocketAddr,
    pub zones: HashMap<String, Vec<ResolverConfig>>,
    pub http: Http,
}

impl Config {
    pub fn from_file<P>(path: P) -> Self
    where
        P: AsRef<Path>,
    {
        let buf = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&buf).unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ResolverConfig {
    Udp(UdpResolver),
    Https(HttpResolver),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UdpResolver {
    pub addr: SocketAddr,
    pub timeout: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpResolver {
    pub url: String,
    pub timeout: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Http {
    pub enabled: bool,
    pub bind: SocketAddr,
}
