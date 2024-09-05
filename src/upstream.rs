pub mod https;
pub mod udp;

use std::io;
use std::time::Duration;

use ahash::HashMap;
use futures::{select_biased, FutureExt};

use crate::proto::{DecodeError, Fqdn, Question, ResourceRecord};

use self::https::HttpsResolver;
use self::udp::UdpResolver;

#[derive(Debug)]
pub enum ResolverError {
    Io(io::Error),
    Timeout,
    Decode(DecodeError),
    NoAnswer,
    Http(reqwest::Error),
}

#[derive(Debug)]
pub enum Resolver {
    Udp(UdpResolver),
    Https(HttpsResolver),
}

impl Resolver {
    pub async fn resolve(&self, question: &Question) -> Result<ResourceRecord, ResolverError> {
        let timeout = tokio::time::sleep(self.timeout()).fuse();
        futures::pin_mut!(timeout);

        match self {
            Self::Udp(resolver) => select_biased! {
                res = resolver.resolve(question).fuse() => res,
                _ = timeout => Err(ResolverError::Timeout),
            },
            Self::Https(resolver) => select_biased! {
                res = resolver.resolve(question).fuse() => res,
                _ = timeout => Err(ResolverError::Timeout),
            },
        }
    }

    pub fn addr(&self) -> String {
        match self {
            Self::Udp(resolver) => resolver.addr.to_string(),
            Self::Https(resolver) => resolver.url.to_string(),
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            Self::Udp(resolver) => resolver.timeout,
            Self::Https(resolver) => resolver.timeout,
        }
    }
}

#[derive(Debug, Default)]
pub struct Zones {
    resolvers: HashMap<Box<str>, Vec<Resolver>>,
}

impl Zones {
    pub fn lookup(&self, fqdn: &Fqdn) -> Option<&[Resolver]> {
        let mut zone = fqdn.as_str();

        loop {
            if let Some(resolvers) = self.resolvers.get(zone) {
                return Some(resolvers);
            }

            if let Some((_, suffix)) = zone.split_once('.') {
                zone = suffix;
                if zone.is_empty() {
                    zone = ".";
                }
            } else {
                return None;
            }
        }
    }

    pub fn insert(&mut self, fqdn: Fqdn, resolver: Resolver) {
        self.resolvers
            .entry(fqdn.0.into_boxed_str())
            .or_default()
            .push(resolver);
    }

    pub fn clear(&mut self) {
        self.resolvers.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::proto::Fqdn;

    use super::Zones;

    #[test]
    fn zones_lookup_exact() {
        let mut zones = Zones::default();
        zones
            .resolvers
            .insert("example.com.".to_owned().into_boxed_str(), Vec::new());

        assert!(zones.lookup(&Fqdn("example.com.".to_owned())).is_some());
    }

    #[test]
    fn zones_lookup_root() {
        let mut zones = Zones::default();
        zones
            .resolvers
            .insert(".".to_owned().into_boxed_str(), Vec::new());

        assert!(zones.lookup(&Fqdn("example.com.".to_owned())).is_some());
    }
}
