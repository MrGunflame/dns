use std::time::{Duration, Instant};

use futures::{FutureExt, select_biased};
use hashbrown::HashMap;
use tokio::sync::Notify;

use crate::cache::{Cache, CacheEntry, Resource, Status};
use crate::config::Upstream;
use crate::metrics::Metrics;
use crate::proto::{Fqdn, Question, RecordData, ResponseCode, Type};
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
    pub async fn resolve(&self, question: &Question) -> Result<Response, ResolverError> {
        let mut resp = Response {
            code: ResponseCode::Ok,
            answers: Vec::new(),
            authority: Vec::new(),
            additional: Vec::new(),
        };

        let mut question_slot = Some(question.clone());
        while let Some(question) = question_slot.take() {
            // If we have an exact match in the cache, return it.
            if let Some(entry) = self
                .cache
                .get(&question.name, question.qtype, question.qclass)
            {
                match entry.status {
                    Status::Ok => {
                        self.metrics.cache_hits_noerror.inc();

                        resp.answers.extend(entry.answers);
                    }
                    Status::NoData => {
                        self.metrics.cache_hits_nodata.inc();

                        resp.authority = entry.authority;
                        resp.additional = entry.additional;
                    }
                    Status::NxDomain => {
                        self.metrics.cache_hits_nxdomain.inc();

                        resp.code = ResponseCode::NameError;
                        resp.answers = entry.answers;
                        resp.authority = entry.authority;
                        resp.additional = entry.additional;

                        // An NXDOMAIN hit is identified by <QNAME, QCLASS>. In other words
                        // there are no records of any type for this QNAME. We don't have to
                        // continue searched for a CNAME record.
                        return Ok(resp);
                    }
                }

                continue;
            }

            // If we fail to find a RR for the requested `question` we
            // have to check whether we have a CNAME record on the FQDN.
            // If we do we need to resolve the FQDN that the CNAME points
            // at and repeat the `question` with the new FQDN.
            // See https://datatracker.ietf.org/doc/html/rfc1034#section-3.6.2
            if question.qtype != Type::CNAME {
                if let Some(entry) = self.cache.get(&question.name, Type::CNAME, question.qclass) {
                    match entry.status {
                        Status::Ok => {
                            self.metrics.cache_hits_noerror.inc();

                            resp.answers.extend(entry.answers);
                        }
                        Status::NoData => {
                            self.metrics.cache_hits_nodata.inc();

                            resp.authority = entry.authority;
                            resp.additional = entry.additional;
                        }
                        // An NXDOMAIN hit is identified by <QNAME, QCLASS>. Since we have already
                        // checked for this above, this can only happen if we have a race condition.
                        // However since we are still using the same <QNAME, QCLASS> we can still use
                        // the response.
                        Status::NxDomain => {
                            self.metrics.cache_hits_nxdomain.inc();

                            resp.code = ResponseCode::NameError;
                            resp.answers = entry.answers;
                            resp.authority = entry.authority;
                            resp.additional = entry.additional;
                            return Ok(resp);
                        }
                    }

                    continue;
                }
            }

            // If we don't have the answer in the cache, resolve it from
            // an origin server.
            // Note that blocking is ok here since if this function is called
            // multiple times, we have a dependency on the previous record
            // and cannot resolve concurrently.
            resp = self.resolve_origin(&question).await?;

            match resp.code {
                ResponseCode::Ok if resp.answers.is_empty() => {
                    self.metrics.cache_misses_nodata.inc();
                }
                ResponseCode::Ok => {
                    self.metrics.cache_misses_noerror.inc();
                }
                ResponseCode::NameError => {
                    self.metrics.cache_misses_nxdomain.inc();
                }
                _ => (),
            }
        }

        Ok(resp)
    }

    async fn resolve_origin(&self, question: &Question) -> Result<Response, ResolverError> {
        let Some(resolvers) = self.zones.lookup(&question.name) else {
            tracing::error!("no nameservers for root zone configured");
            return Err(ResolverError::NoAnswer);
        };

        for resolver in resolvers {
            tracing::debug!("trying upstream {}", resolver.addr());
            let resp = match resolver.resolve(&question).await {
                Ok(answer) => answer,
                Err(ResolverError::NonExistantDomain) => {
                    return Err(ResolverError::NonExistantDomain);
                }
                Err(err) => {
                    tracing::error!("upstream {} failed: {:?}", resolver.addr(), err);
                    continue;
                }
            };

            // It is possible for each RR to contain a different TTL, but such behavior
            // is deprecated in RFC2181.
            // We will choose the lowest TTL value as the TTL value for our cache.
            let mut valid_until = resp.answers.iter().map(|v| v.valid_until).min();

            // NXDOMAIN and NODATA use the MININUM TTL from the attached SOA record.
            if resp.answers.is_empty() || resp.code == ResponseCode::NameError {
                if let Some(data) = resp.authority.iter().find_map(|r| match &r.data {
                    RecordData::SOA(data) => Some(data),
                    _ => None,
                }) {
                    valid_until = Some(Instant::now() + Duration::from_secs(data.minimum.into()));
                }
            }

            if let Some(valid_until) = valid_until {
                let entry = CacheEntry {
                    status: match resp.code {
                        ResponseCode::Ok if resp.answers.is_empty() => Status::NoData,
                        ResponseCode::Ok => Status::Ok,
                        ResponseCode::NameError => Status::NxDomain,
                        // FIXME: Don't do this.
                        _ => Status::Ok,
                    },
                    qname: question.name.clone(),
                    qclass: question.qclass,
                    qtype: question.qtype,
                    expires: valid_until,
                    answers: resp.answers.clone(),
                    additional: resp.additional.clone(),
                    authority: resp.authority.clone(),
                };

                self.metrics.cache_size.add(entry.size_estimate() as u64);
                self.cache.insert(entry);

                self.cache_wakeup.notify_one();
            }

            return Ok(resp);
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

            if let Some(entry) = self.cache.remove_first() {
                self.metrics.cache_size.sub(entry.size_estimate() as u64);
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

#[derive(Clone, Debug)]
pub struct Response {
    pub code: ResponseCode,
    pub answers: Vec<Resource>,
    pub authority: Vec<Resource>,
    pub additional: Vec<Resource>,
}
