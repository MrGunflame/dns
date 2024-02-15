use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tokio::sync::Notify;

use crate::{Class, Question, Type};

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

    pub fn insert(&self, question: Question, resource: Resource) {
        self.expiration
            .write()
            .insert(resource.valid_until, question.clone());
        self.entries.write().insert(question, resource);
        self.wakeup.notify_one();
    }

    pub async fn remove_expiring(&self) -> ! {
        loop {
            let expr = self.expiration.read();
            let Some((timestamp, question)) = expr.first_key_value().map(|(a, b)| (*a, b.clone()))
            else {
                drop(expr);

                // Wait until a entry is available.
                self.wakeup.notified().await;
                continue;
            };
            drop(expr);

            tokio::time::sleep_until((timestamp).into()).await;
            self.entries.write().remove(&question);
            self.expiration.write().pop_first();
        }
    }
}

#[derive(Clone, Debug)]
pub struct Resource {
    pub r#type: Type,
    pub class: Class,
    pub data: Vec<u8>,
    pub valid_until: Instant,
}

impl Resource {
    pub fn ttl(&self) -> Duration {
        self.valid_until - Instant::now()
    }
}
