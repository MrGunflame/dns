use std::collections::HashMap;

use std::io;
use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Read(io::Error),
    #[error(transparent)]
    Parse(toml::de::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub frontend: Frontend,
    pub zones: HashMap<String, Zone>,
    pub http: Http,
}

impl Config {
    pub fn from_file<P>(path: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let buf = std::fs::read_to_string(path).map_err(Error::Read)?;
        toml::from_str(&buf).map_err(Error::Parse)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frontend {
    pub udp: UdpFrontend,
    pub tcp: TcpFrontend,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UdpFrontend {
    pub enable: bool,
    pub bind: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TcpFrontend {
    pub enable: bool,
    pub bind: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Http {
    pub enabled: bool,
    pub bind: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Zone {
    pub zone: String,
    pub upstreams: Vec<Upstream>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum Upstream {
    Udp { addr: SocketAddr },
    Https { url: String, host: Option<String> },
}
