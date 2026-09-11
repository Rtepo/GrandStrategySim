//! Phase 94: Cross-thread seed propagation for deterministic RNG.
//!
//! This module provides functions for propagating the deterministic seed
//! from the main thread to rayon worker threads.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Global atomic seed for cross-thread propagation.
static SEED_ATOMIC: AtomicU64 = AtomicU64::new(0);
static SEED_SET: AtomicBool = AtomicBool::new(false);

/// Set the global deterministic seed and mark it for cross-thread propagation.
pub fn set_seed_global(seed: u64) {
    SEED_ATOMIC.store(seed, Ordering::SeqCst);
    SEED_SET.store(true, Ordering::SeqCst);
    crate::engine::seeded_rng::set_seed(seed);
}

/// Clear the global deterministic seed.
pub fn clear_seed_global() {
    SEED_SET.store(false, Ordering::SeqCst);
    crate::engine::seeded_rng::clear_seed();
}

/// Propagate the global seed to the current rayon worker thread if set.
/// Each thread gets a unique seed derived from the global seed + thread ID
/// to ensure different random values across threads while remaining
/// deterministic.
pub fn ensure_worker_seeded() {
    use std::cell::Cell;
    thread_local! {
        static THREAD_SEEDED: Cell<bool> = const { Cell::new(false) };
    }
    THREAD_SEEDED.with(|seeded| {
        if !seeded.get() && SEED_SET.load(Ordering::SeqCst) {
            let global_seed = SEED_ATOMIC.load(Ordering::SeqCst);
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::thread::current().id().hash(&mut hasher);
            let thread_hash = hasher.finish();
            let thread_seed = global_seed.wrapping_add(thread_hash);
            crate::engine::seeded_rng::set_seed(thread_seed);
            seeded.set(true);
        }
    });
}
