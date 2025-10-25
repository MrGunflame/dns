pub mod https;
pub mod udp;

use std::io;
use std::time::{Duration, Instant};

use futures::{FutureExt, select_biased};
use hashbrown::HashMap;

use crate::cache::Resource;
use crate::proto::{DecodeError, Fqdn, Question, ResourceRecord};
use crate::state::Response;

use self::https::HttpsResolver;
use self::udp::UdpResolver;

#[derive(Debug)]
pub enum ResolverError {
    Io(io::Error),
    Timeout,
    NonExistantDomain,
    Decode(DecodeError),
    NoAnswer,
    Http(reqwest::Error),
    /// The message was too long and was truncated.
    Truncated,
}

#[derive(Debug)]
pub enum Resolver {
    Udp(UdpResolver),
    Https(HttpsResolver),
}

impl Resolver {
    pub async fn resolve(&self, question: &Question) -> Result<Response, ResolverError> {
        let timeout = tokio::time::sleep(self.timeout()).fuse();
        futures::pin_mut!(timeout);

        let packet = match self {
            Self::Udp(resolver) => select_biased! {
                res = resolver.resolve(question).fuse() => res?,
                _ = timeout => return Err(ResolverError::Timeout),
            },
            Self::Https(resolver) => select_biased! {
                res = resolver.resolve(question).fuse() => res?,
                _ = timeout => return Err(ResolverError::Timeout),
            },
        };

        let map_rr_to_res = |rr: ResourceRecord| Resource {
            name: rr.name,
            r#type: rr.r#type,
            class: rr.class,
            data: rr.rdata,
            valid_until: Instant::now() + Duration::from_secs(rr.ttl.into()),
        };

        Ok(Response {
            code: packet.response_code,
            answers: packet.answers.into_iter().map(map_rr_to_res).collect(),
            authority: packet.authority.into_iter().map(map_rr_to_res).collect(),
            additional: packet.additional.into_iter().map(map_rr_to_res).collect(),
        })
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
    resolvers: HashMap<Box<[u8]>, Vec<Resolver>>,
}

impl Zones {
    pub fn lookup(&self, fqdn: &Fqdn) -> Option<&[Resolver]> {
        let mut zone = fqdn.as_bytes();

        loop {
            if let Some(resolvers) = self.resolvers.get(zone) {
                return Some(resolvers);
            }

            if let Some(index) = memchr::memchr(b'.', zone) {
                let (_, rem) = zone.split_at(index + 1);
                zone = rem;
                if zone.is_empty() {
                    zone = b".";
                }
            } else {
                return None;
            }
        }
    }

    pub fn insert(&mut self, fqdn: Fqdn, resolver: Resolver) {
        self.resolvers
            .entry(fqdn.0.into_boxed_slice())
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
            .insert(b"example.com.".to_vec().into_boxed_slice(), Vec::new());

        assert!(zones.lookup(&Fqdn(b"example.com.".to_vec())).is_some());
    }

    #[test]
    fn zones_lookup_root() {
        let mut zones = Zones::default();
        zones
            .resolvers
            .insert(b".".to_vec().into_boxed_slice(), Vec::new());

        assert!(zones.lookup(&Fqdn(b"example.com.".to_vec())).is_some());
    }
}
