use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tokio::sync::Notify;

use crate::proto::{Class, Fqdn, Question, RecordData, Type};

#[derive(Debug, Default)]
pub struct Cache {
    entries: RwLock<HashMap<Question, Resource>>,
    expiration: RwLock<BTreeMap<Instant, Question>>,
    wakeup: Notify,
}

impl Cache {
    pub fn get(&self, question: &Question) -> Option<Resource> {
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
        self.entries.write().insert(question, resource);
        self.wakeup.notify_one();
    }

    pub fn remove_first(&self) -> Option<Resource> {
        if let Some((_, question)) = self.expiration.write().pop_first() {
            self.entries.write().remove(&question)
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
