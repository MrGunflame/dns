use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hashbrown::HashMap;
use parking_lot::RwLock;

#[derive(Debug, Default)]
pub struct Metrics {
    pub cache_hits_noerror: Counter,
    pub cache_misses_noerror: Counter,
    pub cache_size: Gauge,
    pub resolve_time: Histogram,
    pub upstream_times: HashMap<String, Histogram>,
}

#[derive(Debug, Default)]
pub struct Gauge(AtomicU64);

impl Gauge {
    pub fn add(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    pub fn sub(&self, n: u64) {
        self.0.fetch_sub(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default)]
pub struct Histogram {
    pub buckets: RwLock<HashMap<u32, Counter>>,
}

impl Histogram {
    pub fn insert(&self, value: Duration) {
        let bucket = value.as_nanos().ilog2();

        let buckets = self.buckets.read();
        if let Some(counter) = buckets.get(&bucket) {
            counter.inc();
            return;
        }

        drop(buckets);
        let mut buckets = self.buckets.write();
        buckets.entry(bucket).or_default().inc();
    }
}
