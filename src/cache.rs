use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::{Class, Question, Type};

#[derive(Clone, Debug, Default)]
pub struct Cache {
    entries: HashMap<Question, Resource>,
}

impl Cache {
    pub fn get(&self, question: &Question) -> Option<Resource> {
        self.entries.get(question).cloned()
    }

    pub fn insert(&mut self, question: Question, resource: Resource) {
        self.entries.insert(question, resource);
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
