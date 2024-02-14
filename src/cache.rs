use std::collections::{BTreeMap, HashMap};
use std::task::Poll;
use std::time::{Duration, Instant};

use crate::{Class, Question, Type};

#[derive(Clone, Debug, Default)]
pub struct Cache {
    entries: HashMap<Question, Resource>,
    expiration: BTreeMap<Instant, Question>,
}

impl Cache {
    pub fn get(&self, question: &Question) -> Option<Resource> {
        self.entries.get(question).cloned()
    }

    pub fn insert(&mut self, question: Question, resource: Resource) {
        self.expiration
            .insert(resource.valid_until, question.clone());
        self.entries.insert(question, resource);
    }

    pub async fn remove_expiring(&mut self) -> ! {
        loop {
            let Some((timestamp, question)) = self.expiration.first_key_value() else {
                futures::future::poll_fn(|_| Poll::<()>::Pending).await;
                unreachable!()
            };

            tokio::time::sleep_until((*timestamp).into()).await;
            self.entries.remove(&question);
            self.expiration.pop_first();
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
