use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use hashbrown::HashMap;
use parking_lot::RwLock;
use tokio::sync::Notify;

use crate::proto::{Class, Fqdn, Question, RecordData, Type};

#[derive(Debug, Default)]
pub struct Cache {
    entries: RwLock<HashMap<Question, Vec<Resource>>>,
    expiration: RwLock<BTreeMap<Instant, Question>>,
    wakeup: Notify,
}

impl Cache {
    pub fn get(&self, question: &Question) -> Option<Vec<Resource>> {
        self.entries.read().get(question).cloned()
    }

    pub fn insert(&self, resource: Resource) {
        let question = Question {
            name: resource.name.clone(),
            qtype: resource.r#type,
            qclass: resource.class,
        };

        self.expiration
            .write()
            .insert(resource.valid_until, question.clone());
        self.entries
            .write()
            .entry(question)
            .or_default()
            .push(resource);
        self.wakeup.notify_one();
    }

    pub fn remove_first(&self) -> Option<Vec<Resource>> {
        if let Some((valid_until, question)) = self.expiration.write().pop_first() {
            let mut removed_res = Vec::new();

            let mut entries = self.entries.write();
            let entry = entries.get_mut(&question).unwrap();

            let mut next_expires = Instant::now();

            entry.retain(|res| {
                if res.valid_until == valid_until {
                    removed_res.push(res.clone());
                    false
                } else {
                    next_expires = std::cmp::max(next_expires, res.valid_until);
                    true
                }
            });

            if entry.is_empty() {
                entries.remove(&question);
            }

            Some(removed_res)
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
