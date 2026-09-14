//! Phase 94: Cross-thread seed propagation for deterministic RNG.
//!
//! This module provides functions for propagating the deterministic seed
//! from the main thread to rayon worker threads.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Global atomic seed for cross-thread propagation.
static SEED_ATOMIC: AtomicU64 = AtomicU64::new(0);
static SEED_SET: AtomicBool = AtomicBool::new(false);

/// Thread-local flag: true on the main thread (the thread that called
/// `set_seed_global`). The main thread's RNG is managed by `set_seed_global`
/// and should NOT be reset by `ensure_worker_seeded`, because rayon may
/// schedule parallel tasks on the main thread, and resetting its RNG
/// would corrupt the deterministic sequence for main-thread code that
/// runs between parallel sections.
thread_local! {
    static IS_MAIN_THREAD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Set the global deterministic seed and mark it for cross-thread propagation.
/// Must be called from the main thread.
pub fn set_seed_global(seed: u64) {
    SEED_ATOMIC.store(seed, Ordering::SeqCst);
    SEED_SET.store(true, Ordering::SeqCst);
    crate::engine::seeded_rng::set_seed(seed);
    // Mark the current thread as the main thread.
    IS_MAIN_THREAD.with(|flag| flag.set(true));
}

/// Clear the global deterministic seed.
pub fn clear_seed_global() {
    SEED_SET.store(false, Ordering::SeqCst);
    crate::engine::seeded_rng::clear_seed();
    IS_MAIN_THREAD.with(|flag| flag.set(false));
}

/// Propagate the global seed to the current rayon worker thread if set.
///
/// Phase 94: For full determinism, re-seed at every call using the global
/// seed directly (no thread-ID hash). Thread IDs are non-deterministic
/// across runs, causing intermittent M0 conservation failures. By
/// re-seeding with the same global seed at the start of every parallel
/// task, each task gets a deterministic RNG state regardless of which
/// rayon worker thread executes it or how many prior tasks ran on
/// that thread.
///
/// IMPORTANT: The main thread is skipped because its RNG is managed by
/// `set_seed_global` and advanced by main-thread code between parallel
/// sections. Resetting it here (when rayon schedules a task on the main
/// thread) would corrupt the deterministic sequence for subsequent
/// main-thread code.
pub fn ensure_worker_seeded() {
    if SEED_SET.load(Ordering::SeqCst) {
        // Skip the main thread — its RNG is managed separately.
        let is_main = IS_MAIN_THREAD.with(|flag| flag.get());
        if !is_main {
            let global_seed = SEED_ATOMIC.load(Ordering::SeqCst);
            crate::engine::seeded_rng::set_seed(global_seed);
        }
    }
}
