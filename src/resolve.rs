use std::time::{Duration, Instant};

use crate::cache::{Cache, Resource};
use crate::upstream::{ResolverError, Resolvers};
use crate::Question;

pub struct ResolverQueue {
    pub cache: Cache,
    pub upstream: Resolvers,
}

impl ResolverQueue {
    pub async fn resolve(&mut self, question: &Question) -> Result<Resource, ResolverError> {
        if let Some(answer) = self.cache.get(&question) {
            return Ok(answer.clone());
        }

        let answer = self.upstream.resolve(&question).await?;
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
}
