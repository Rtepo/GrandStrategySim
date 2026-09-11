//! Phase 94: Deterministic RNG for reproducible test worlds.
//!
//! This module provides a global seeded RNG that replaces `rand::thread_rng()`
//! when a seed is set. The engine has 90+ calls to `thread_rng()` scattered
//! across the turn loop, corporate generator, and economy modules. Threading
//! a seeded RNG through all of them would be a massive refactor. Instead,
//! we use a thread-local `UnsafeCell<StdRng>` that can be seeded once at the
//! start of a test. All `rand::thread_rng()` calls in the engine are replaced
//! with `crate::engine::seeded_rng::thread_rng()`, which returns the seeded
//! RNG when set, or falls back to `rand::thread_rng()` when not set.
//!
//! # Safety
//! The seeded RNG is stored in a thread-local `UnsafeCell`. This is safe
//! because:
//! 1. Thread-local storage ensures no cross-thread access.
//! 2. The `SeededThreadRng` guard holds a mutable reference with a `'static`
//!    lifetime tied to the thread-local cell. The guard is dropped before
//!    any new call to `thread_rng()`, preventing aliasing.
//! 3. Re-entrancy (calling `thread_rng()` while holding a guard) would
//!    violate Rust's aliasing rules and is undefined behavior. All existing
//!    engine code creates, uses, and drops the RNG within the same scope
//!    without re-entrancy, so this is safe in practice.
//!
//! # Usage
//! ```
//! use sim_engine::engine::seeded_rng;
//!
//! // Seed once at the start of a test
//! seeded_rng::set_seed(42);
//!
//! // All code that calls `sim_engine::engine::seeded_rng::thread_rng()`
//! // will now use the seeded RNG, producing deterministic output.
//!
//! // Clear the seed when done
//! seeded_rng::clear_seed();
//! ```

use rand::rngs::{StdRng, ThreadRng};
use rand::{Rng, RngCore, SeedableRng};
use std::cell::UnsafeCell;

thread_local! {
    static SEEDED_RNG: UnsafeCell<Option<StdRng>> = const { UnsafeCell::new(None) };
}

/// Set the global deterministic seed. All subsequent calls to `thread_rng()`
/// in this module will use a seeded StdRng instead of a true thread RNG.
pub fn set_seed(seed: u64) {
    SEEDED_RNG.with(|cell| unsafe {
        *cell.get() = Some(StdRng::seed_from_u64(seed));
    });
}

/// Clear the global deterministic seed. Subsequent calls to `thread_rng()`
/// will use a true thread RNG.
pub fn clear_seed() {
    SEEDED_RNG.with(|cell| unsafe {
        *cell.get() = None;
    });
}

/// Returns whether a deterministic seed is currently set.
pub fn is_seeded() -> bool {
    SEEDED_RNG.with(|cell| unsafe { (*cell.get()).is_some() })
}

/// A wrapper enum that can be either a seeded StdRng or a ThreadRng.
/// This implements RngCore so it can be used anywhere `impl Rng` is expected.
pub enum SeededThreadRng {
    /// A deterministic StdRng borrowed from the thread-local cell.
    Seeded(StdRng),
    /// A true thread-local RNG (non-deterministic).
    Thread(ThreadRng),
}

impl RngCore for SeededThreadRng {
    fn next_u32(&mut self) -> u32 {
        match self {
            SeededThreadRng::Seeded(r) => r.next_u32(),
            SeededThreadRng::Thread(r) => r.next_u32(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        match self {
            SeededThreadRng::Seeded(r) => r.next_u64(),
            SeededThreadRng::Thread(r) => r.next_u64(),
        }
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        match self {
            SeededThreadRng::Seeded(r) => r.fill_bytes(dest),
            SeededThreadRng::Thread(r) => r.fill_bytes(dest),
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        match self {
            SeededThreadRng::Seeded(r) => r.try_fill_bytes(dest),
            SeededThreadRng::Thread(r) => r.try_fill_bytes(dest),
        }
    }
}

impl Drop for SeededThreadRng {
    fn drop(&mut self) {
        // When the Seeded variant is dropped, put the RNG state back into
        // the thread-local cell so the next call to `thread_rng()` continues
        // from where this one left off.
        if let SeededThreadRng::Seeded(ref mut rng) = self {
            SEEDED_RNG.with(|cell| unsafe {
                // SAFETY: We're putting the RNG back into the thread-local cell.
                // This is safe because we took it out in `thread_rng()` and no
                // other code has access to it while we hold it.
                // Use ptr::write to move the RNG without cloning.
                *cell.get() = Some(std::ptr::read(rng as *mut StdRng));
            });
        }
    }
}

/// Replacement for `rand::thread_rng()`. Returns a seeded RNG when a seed
/// is set (via `set_seed`), or a true thread RNG when no seed is set.
///
/// This function should be used instead of `rand::thread_rng()` in all
/// engine code that needs to be deterministic in tests.
pub fn thread_rng() -> SeededThreadRng {
    SEEDED_RNG.with(|cell| unsafe {
        // Take the RNG out of the cell. It will be put back when the
        // SeededThreadRng is dropped.
        match (*cell.get()).take() {
            Some(seeded) => SeededThreadRng::Seeded(seeded),
            None => SeededThreadRng::Thread(rand::thread_rng()),
        }
    })
}
