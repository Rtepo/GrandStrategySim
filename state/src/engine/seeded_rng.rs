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
///
/// The `Seeded` variant holds a raw pointer INTO the thread-local cell rather
/// than the StdRng itself. Taking the RNG out of the cell (the old design)
/// meant any nested `thread_rng()` call while a guard was alive observed an
/// empty cell and silently fell back to OS entropy — making every function
/// that created its own guard (`generate_regional_topology`, name
/// generators, `assign_regional_heads`, …) nondeterministic whenever a
/// caller also held a guard. Pointing into the cell lets nested guards share
/// the same advancing stream: re-entrant draws are deterministic.
pub enum SeededThreadRng {
    /// A deterministic StdRng shared via pointer into the thread-local cell.
    Seeded(*mut StdRng),
    /// A true thread-local RNG (non-deterministic).
    Thread(ThreadRng),
}

/// Global counter of seeded RNG draws. Diagnostic-only: lets stage-level
/// fingerprints pinpoint exactly where two runs' streams diverge.
static DRAW_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Total seeded draws since process start (or last `reset_draw_count`).
pub fn draw_count() -> u64 {
    DRAW_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Reset the draw counter (e.g., at generation start).
pub fn reset_draw_count() {
    DRAW_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[inline]
fn bump_draw() {
    DRAW_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

impl RngCore for SeededThreadRng {
    fn next_u32(&mut self) -> u32 {
        bump_draw();
        match self {
            // SAFETY: The pointer references the StdRng inside the
            // thread-local `UnsafeCell<Option<StdRng>>`. Each call creates a
            // transient &mut that ends before returning, so nested guards
            // never hold overlapping live &mut borrows — the same interior-
            // mutability pattern the cell was introduced for.
            SeededThreadRng::Seeded(ptr) => unsafe { (**ptr).next_u32() },
            SeededThreadRng::Thread(r) => r.next_u32(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        bump_draw();
        match self {
            SeededThreadRng::Seeded(ptr) => unsafe { (**ptr).next_u64() },
            SeededThreadRng::Thread(r) => r.next_u64(),
        }
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        bump_draw();
        match self {
            SeededThreadRng::Seeded(ptr) => unsafe { (**ptr).fill_bytes(dest) },
            SeededThreadRng::Thread(r) => r.fill_bytes(dest),
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        bump_draw();
        match self {
            SeededThreadRng::Seeded(ptr) => unsafe { (**ptr).try_fill_bytes(dest) },
            SeededThreadRng::Thread(r) => r.try_fill_bytes(dest),
        }
    }
}

/// Replacement for `rand::thread_rng()`. Returns a seeded RNG when a seed
/// is set (via `set_seed`), or a true thread RNG when no seed is set.
///
/// This function should be used instead of `rand::thread_rng()` in all
/// engine code that needs to be deterministic in tests.
///
/// Re-entrant by design: multiple live guards all point at the same
/// thread-local StdRng and advance the shared stream — the caller that
/// happens to draw next gets the next stream value, deterministically.
pub fn thread_rng() -> SeededThreadRng {
    SEEDED_RNG.with(|cell| unsafe {
        // Borrow the RNG in-place inside the cell. Do NOT take() it out —
        // see the `SeededThreadRng` doc comment for why take-out silently
        // produced OS entropy on nested calls.
        match &mut *cell.get() {
            Some(seeded) => SeededThreadRng::Seeded(seeded as *mut StdRng),
            None => SeededThreadRng::Thread(rand::thread_rng()),
        }
    })
}
