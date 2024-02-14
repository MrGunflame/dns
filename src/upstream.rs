pub mod https;
pub mod udp;

use std::collections::HashMap;
use std::io;

use crate::{DecodeError, Fqdn, Question, ResourceRecord};

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
        match self {
            Self::Udp(resolver) => resolver.resolve(question).await,
            Self::Https(resolver) => resolver.resolve(question).await,
        }
    }
}

#[derive(Debug, Default)]
pub struct Zones {
    resolvers: HashMap<String, Vec<Resolver>>,
}

impl Zones {
    pub fn lookup(&self, fqdn: &Fqdn) -> Option<&[Resolver]> {
        let mut zone = fqdn.0.as_str();

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
        self.resolvers.entry(fqdn.0).or_default().push(resolver);
    }

    pub fn clear(&mut self) {
        self.resolvers.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::Fqdn;

    use super::Zones;

    #[test]
    fn zones_lookup_exact() {
        let mut zones = Zones::default();
        zones
            .resolvers
            .insert("example.com.".to_owned(), Vec::new());

        assert!(zones.lookup(&Fqdn("example.com.".to_owned())).is_some());
    }

    #[test]
    fn zones_lookup_root() {
        let mut zones = Zones::default();
        zones.resolvers.insert(".".to_owned(), Vec::new());

        assert!(zones.lookup(&Fqdn("example.com.".to_owned())).is_some());
    }
}
