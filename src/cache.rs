use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use hashbrown::HashMap;
use parking_lot::RwLock;

use crate::proto::{Class, Fqdn, RecordData, Type};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Ok,
    NxDomain,
    NoData,
}

#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub status: Status,
    pub qname: Fqdn,
    pub qtype: Type,
    pub qclass: Class,
    pub expires: Instant,
    pub answers: Vec<Resource>,
    pub authority: Vec<Resource>,
    pub additional: Vec<Resource>,
}

impl CacheEntry {
    /// Returns an estimate of the memory size of this entry.
    pub fn size_estimate(&self) -> usize {
        let mut size = size_of::<Self>();
        size += self.qname.as_bytes().len();

        for res in self
            .answers
            .iter()
            .chain(self.authority.iter())
            .chain(self.additional.iter())
        {
            size += res.name.as_bytes().len();
            size += usize::from(res.data.len());
        }

        size
    }
}

#[derive(Debug, Default)]
pub struct Cache {
    entries: RwLock<HashMap<(Fqdn, Class), DomainState>>,
    expiration: RwLock<BTreeMap<Instant, (Fqdn, Class, Type)>>,
}

#[derive(Clone, Debug)]
enum DomainState {
    Existant(HashMap<Type, CacheEntry>),
    NonExistant(CacheEntry),
}

impl Cache {
    pub fn get(&self, qname: &Fqdn, qtype: Type, qclass: Class) -> Option<CacheEntry> {
        let entries = self.entries.read();

        match entries.get(&(qname.clone(), qclass))? {
            DomainState::Existant(e) => e.get(&qtype).cloned(),
            DomainState::NonExistant(e) => Some(e.clone()),
        }
    }

    pub fn insert(&self, entry: CacheEntry) {
        let mut entries = self.entries.write();

        let e = entries.entry((entry.qname.clone(), entry.qclass));

        self.expiration.write().insert(
            entry.expires,
            (entry.qname.clone(), entry.qclass, entry.qtype),
        );

        if entry.status == Status::NxDomain {
            e.insert(DomainState::NonExistant(entry));
        } else {
            let state = e.or_insert_with(|| DomainState::Existant(HashMap::new()));

            match state {
                DomainState::Existant(map) => {
                    map.insert(entry.qtype, entry);
                }
                DomainState::NonExistant(_) => {
                    let mut map = HashMap::new();
                    map.insert(entry.qtype, entry);
                    *state = DomainState::Existant(map);
                }
            }
        }
    }

    pub fn remove_first(&self) -> Option<CacheEntry> {
        if let Some((valid_until, (qname, qclass, qtype))) = self.expiration.write().pop_first() {
            let mut entries = self.entries.write();
            let Some(entry) = entries.get_mut(&(qname.clone(), qclass)) else {
                return None;
            };

            match entry {
                DomainState::NonExistant(e) => {
                    if valid_until != e.expires {
                        return None;
                    }

                    let DomainState::NonExistant(e) = entries.remove(&(qname, qclass)).unwrap()
                    else {
                        unreachable!()
                    };

                    Some(e)
                }
                DomainState::Existant(map) => {
                    // If a cache record get overwritten we don't update
                    // the expiration timer queue. The time we have may
                    // no longer be valid.
                    if map.get(&qtype).is_none_or(|v| valid_until == v.expires) {
                        return None;
                    }

                    let e = map.remove(&qtype).unwrap();
                    if map.is_empty() {
                        entries.remove(&(qname, qclass));
                    }

                    Some(e)
                }
            }
        } else {
            None
        }
    }

    pub fn next_expiration(&self) -> Option<Instant> {
        let expr = self.expiration.read();
        expr.first_key_value().map(|(v, _)| *v)
    }
}

#[derive(Clone, Debug)]
pub struct Resource {
    pub name: Fqdn,
    pub r#type: Type,
    pub class: Class,
    pub data: RecordData,
    pub valid_until: Instant,
}

impl Resource {
    pub fn ttl(&self) -> Duration {
        self.valid_until - Instant::now()
    }
}
