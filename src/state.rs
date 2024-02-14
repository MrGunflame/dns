use std::time::{Duration, Instant};

use reqwest::Url;

use crate::cache::{Cache, Resource};
use crate::config::Config;
use crate::upstream::https::HttpsResolver;
use crate::upstream::udp::UdpResolver;
use crate::upstream::{Resolver, ResolverError, Zones};
use crate::{Fqdn, Question};

pub struct State {
    pub cache: Cache,
    pub zones: Zones,
    pub config: Config,
}

impl State {
    pub async fn resolve(&mut self, question: &Question) -> Result<Resource, ResolverError> {
        if let Some(answer) = self.cache.get(&question) {
            tracing::info!("using cached result (valid for {:?})", answer.ttl());
            return Ok(answer.clone());
        }

        let Some(resolvers) = self.zones.lookup(&question.name) else {
            tracing::error!("no nameservers for root zone configured");
            return Err(ResolverError::NoAnswer);
        };
        let resolver = resolvers.first().unwrap();

        tracing::info!("looking up query");
        let answer = resolver.resolve(&question).await?;
        let res = Resource {
            r#type: answer.r#type,
            class: answer.class,
            data: answer.rddata,
            valid_until: Instant::now() + Duration::from_secs(answer.ttl as u64),
        };

        if answer.ttl != 0 {
            self.cache.insert(question.clone(), res.clone());
        }

        Ok(res)
    }

    pub fn generate_zones(&mut self) {
        self.zones.clear();

        for (zone, resolvers) in &self.config.zones {
            for resolver in resolvers {
                let resolver = match resolver {
                    crate::config::ResolverConfig::Udp(conf) => Resolver::Udp(UdpResolver::new(
                        conf.addr,
                        Duration::from_secs(conf.timeout),
                    )),
                    crate::config::ResolverConfig::Https(conf) => {
                        Resolver::Https(HttpsResolver::new(
                            Url::parse(&conf.url).unwrap(),
                            Duration::from_secs(conf.timeout),
                        ))
                    }
                };

                self.zones.insert(Fqdn(zone.clone()), resolver);
            }
        }
    }
}
