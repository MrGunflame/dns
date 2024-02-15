use std::sync::atomic::AtomicU64;

#[derive(Debug, Default)]
pub struct Metrics {
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub cache_size: AtomicU64,
}
