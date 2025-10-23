use std::time::{Duration, Instant};

use futures::{FutureExt, select_biased};
use hashbrown::HashMap;
use tokio::sync::Notify;

use crate::cache::{Cache, Resource};
use crate::config::Upstream;
use crate::metrics::Metrics;
use crate::proto::{Fqdn, Question, RecordData, Type};
use crate::upstream::https::HttpsResolver;
use crate::upstream::udp::UdpResolver;
use crate::upstream::{Resolver, ResolverError, Zones};

const TIMEOUT: Duration = Duration::from_secs(4);

pub struct State {
    pub cache: Cache,
    pub zones: Zones,
    pub metrics: Metrics,
    cache_wakeup: Notify,
}

impl State {
    pub fn new(zones: HashMap<String, Vec<Upstream>>) -> Self {
        let zones = generate_zones(&zones);

        Self {
            cache: Cache::default(),
            zones,
            cache_wakeup: Notify::default(),
            metrics: Metrics::default(),
        }
    }

    /// Resolve a single [`Question`].
    pub async fn resolve(&self, question: &Question) -> Result<Vec<Resource>, ResolverError> {
        let mut answers = Vec::new();

        let mut question_slot = Some(question.clone());
        while let Some(question) = question_slot.take() {
            // If we have an exact match in the cache, return it.
            if let Some(answer) = self.cache.get(&question) {
                self.metrics.cache_hits_noerror.inc();
                for answer in &answer {
                    tracing::debug!("using cached result (valid for {:?})", answer.ttl());
                }

                answers.extend(answer);
                continue;
            }

            // If we fail to find a RR for the requested `question` we
            // have to check whether we have a CNAME record on the FQDN.
            // If we do we need to resolve the FQDN that the CNAME points
            // at and repeat the `question` with the new FQDN.
            // See https://datatracker.ietf.org/doc/html/rfc1034#section-3.6.2
            if question.qtype != Type::CNAME {
                if let Some(answer) = self.cache.get(&Question {
                    name: question.name.clone(),
                    qtype: Type::CNAME,
                    qclass: question.qclass,
                }) {
                    for answer in answer {
                        let origin = match &answer.data {
                            RecordData::CNAME(fqdn) => fqdn.clone(),
                            _ => continue,
                        };

                        answers.push(answer);
                        question_slot = Some(Question {
                            name: origin,
                            qtype: question.qtype,
                            qclass: question.qclass,
                        });
                    }

                    continue;
                }
            }

            // If we don't have the answer in the cache, resolve it from
            // an origin server.
            // Note that blocking is ok here since if this function is called
            // multiple times, we have a dependency on the previous record
            // and cannot resolve concurrently.
            answers.extend(self.resolve_origin(&question).await?);
            self.metrics.cache_misses_noerror.inc();
        }

        Ok(answers)
    }

    async fn resolve_origin(&self, question: &Question) -> Result<Vec<Resource>, ResolverError> {
        let Some(resolvers) = self.zones.lookup(&question.name) else {
            tracing::error!("no nameservers for root zone configured");
            return Err(ResolverError::NoAnswer);
        };

        for resolver in resolvers {
            tracing::debug!("trying upstream {}", resolver.addr());
            let answers = match resolver.resolve(&question).await {
                Ok(answer) => answer,
                Err(ResolverError::NonExistantDomain) => {
                    return Err(ResolverError::NonExistantDomain);
                }
                Err(err) => {
                    tracing::error!("upstream {} failed: {:?}", resolver.addr(), err);
                    continue;
                }
            };

            let mut resources = Vec::new();
            for answer in answers {
                let res = Resource {
                    name: answer.name,
                    r#type: answer.r#type,
                    class: answer.class,
                    data: answer.rdata.into(),
                    valid_until: Instant::now() + Duration::from_secs(answer.ttl.into()),
                };

                if answer.ttl != 0 {
                    self.cache.insert(res.clone());
                    self.cache_wakeup.notify_one();
                    self.metrics.cache_size.add(res.data.len() as u64);
                }

                resources.push(res);
            }

            return Ok(resources);
        }

        Err(ResolverError::NoAnswer)
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
                for record in record {
                    self.metrics.cache_size.sub(record.data.len() as u64);
                }
            }
        }
    }
}

fn generate_zones(input: &HashMap<String, Vec<Upstream>>) -> Zones {
    let mut zones = Zones::default();

    for (zone, resolvers) in input {
        for resolver in resolvers {
            let resolver = match resolver {
                Upstream::Udp { addr } => Resolver::Udp(UdpResolver::new(*addr, TIMEOUT)),
                Upstream::Https { url, host } => Resolver::Https(
                    HttpsResolver::new(&url, host.as_ref().map(|v| v.as_str()), TIMEOUT).unwrap(),
                ),
            };

            zones.insert(Fqdn::new_unchecked(zone.clone()), resolver);
        }
    }

    zones
}
