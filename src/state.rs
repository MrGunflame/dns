use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use futures::{select_biased, FutureExt};
use reqwest::Url;
use tokio::sync::Notify;

use crate::cache::{Cache, Resource};
use crate::config::Config;
use crate::metrics::Metrics;
use crate::proto::{Fqdn, Question};
use crate::upstream::https::HttpsResolver;
use crate::upstream::udp::UdpResolver;
use crate::upstream::{Resolver, ResolverError, Zones};

pub struct State {
    pub cache: Cache,
    pub zones: Zones,
    pub config: Config,
    pub metrics: Metrics,
    cache_wakeup: Notify,
}

impl State {
    pub fn new(config: Config) -> Self {
        let mut this = Self {
            cache: Cache::default(),
            zones: Zones::default(),
            cache_wakeup: Notify::default(),
            metrics: Metrics::default(),
            config,
        };
        this.generate_zones();
        this
    }

    /// Resolve a single [`Question`].
    pub async fn resolve(&self, question: &Question) -> Result<Resource, ResolverError> {
        if let Some(answer) = self.cache.get(&question) {
            self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
            tracing::info!("using cached result (valid for {:?})", answer.ttl());
            return Ok(answer.clone());
        }

        let Some(resolvers) = self.zones.lookup(&question.name) else {
            tracing::error!("no nameservers for root zone configured");
            return Err(ResolverError::NoAnswer);
        };

        for resolver in resolvers {
            tracing::debug!("trying upstream {}", resolver.addr());
            let answer = match resolver.resolve(&question).await {
                Ok(answer) => answer,
                Err(err) => {
                    tracing::error!("upstream {} failed: {:?}", resolver.addr(), err);
                    continue;
                }
            };

            let res = Resource {
                r#type: answer.r#type,
                class: answer.class,
                data: answer.rddata,
                valid_until: Instant::now() + Duration::from_secs(answer.ttl.into()),
            };

            if answer.ttl != 0 {
                self.cache.insert(question.clone(), res.clone());
                self.cache_wakeup.notify_one();
                self.metrics
                    .cache_size
                    .fetch_add(res.data.len() as u64, Ordering::Relaxed);
            }

            return Ok(res);
        }

        Err(ResolverError::NoAnswer)
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

                self.zones
                    .insert(Fqdn::new_unchecked(zone.clone()), resolver);
            }
        }
    }

    pub async fn cleanup(&self) -> ! {
        loop {
            let Some(instant) = self.cache.next_expiration() else {
                self.cache_wakeup.notified().await;
                continue;
            };

            // While sleeping it is possible that a new entry with
            // a shorter TTL gets inserted. In this case we must
            // interrupt the current sleep to ensure we always sleep
            // on the next expiration time.
            select_biased! {
                _ = self.cache_wakeup.notified().fuse() => continue,
                _ = tokio::time::sleep_until(instant.into()).fuse() => (),
            }

            if let Some(record) = self.cache.remove_first() {
                self.metrics
                    .cache_size
                    .fetch_sub(record.data.len() as u64, Ordering::Relaxed);
            }
        }
    }
}
