# Architecture v4: Data-Driven Testing Framework

**Version:** v4.0-draft (patched v4.0.1)
**Date:** 2026-09-07
**Author:** Agent 5 (Lead System Manager)
**Status:** Specification — pending implementation approval

### v4.0.1 Patches (Integration Flaws)

Three integration flaws were identified during review and patched in this
revision:

1. **CI Strict Mode (§4.5, §6.1):** `export CI=true` MUST be set before
   `cargo nextest` so `cargo-insta` fails hard on snapshot mismatches
   instead of silently writing `.snap.new` files or hanging on prompts.
2. **Proper Cargo Test Targets (§4.4, §4.6):** Replaced `#[cfg(feature)]`
   inside `.rs` files with explicit `[[test]]` blocks in `state/Cargo.toml`
   using `required-features = ["epic-tests"]`. This prevents empty test
   binaries from confusing nextest during fast runs.
3. **Artifact Routing (§4.5, §6.1):** When the daemon successfully runs
   epic tests, it extracts the absolute path to `diagnostic_output/` and
   embeds it in the `AUDIT_REQUESTED` event payload so Agent 4 can locate
   the JSON dumps for Python validation.

---

## 0. Executive Summary

Architecture v4 overhauls the testing infrastructure for the SillyElaborateState
macroeconomic engine. The codebase has reached **2,199 tests** (1,500 inline +
699 integration across 34 files), with **1,647 `assert_eq!` calls** and **2,926
`assert!` calls**. Workspace line coverage is **70.50%** (47,731 / 67,702 lines).

Three problems motivate this overhaul:

1. **Maintenance burden:** Three epic test files alone account for 3,816 lines
   and 303 tests (`waste_epic`, `sanitation_epic`, `thermal_epic`). These
   choke the fast CI/CD pipeline despite testing stable mechanics.
2. **Brittle assertions:** Complex state outputs are verified with hand-written
   `assert_eq!` blocks that break on any cosmetic change to struct field order,
  float formatting, or HashMap iteration order.
3. **Auditor blind spots:** Critical diagnostic tests (M0 conservation, mass
   conservation, bank balance-sheet identity) run inside Rust and emit
   pass/fail booleans. Agent 4 (Auditor) cannot independently verify the
   double-entry invariants without reading Rust source code.

v4 addresses these with **snapshot testing** (cargo-insta), **test
restructuring** (epic segregation), and **auditor data analytics** (JSON
ledger dumps + external Python verification).

---

## 1. Coverage Analysis (cargo-tarpaulin)

### 1.1 Methodology

```bash
cargo install cargo-tarpaulin --locked
cargo tarpaulin --ignore-tests --workspace --out Stdout --skip-clean
```

`--ignore-tests` excludes test harness code from the denominator, measuring
only source-line coverage. `--skip-clean` reuses cached builds.

### 1.2 Aggregate Results

| Metric | Value |
|---|---|
| **Workspace coverage** | 70.50% |
| **Lines covered** | 47,731 / 67,702 |
| **Inline tests** (state/src/) | 1,500 |
| **Integration tests** (state/tests/) | 699 |
| **Total tests** | 2,199 |
| **`assert_eq!` calls** | 1,647 |
| **`assert!` calls** | 2,926 |
| **Integration test files** | 34 |

### 1.3 Zero-Coverage Modules (0/N lines)

These modules have source code but zero executed lines under `--ignore-tests`:

| Module | Lines | Domain |
|---|---|---|
| `economy/pension.rs` | 307 | Pension system |
| `infrastructure/effects.rs` | 158 | Infrastructure effect propagation |
| `utilities/demand.rs` | 184 | Demand modeling |
| `state/special_economic_zones.rs` | 145 | SEZ logic |
| `state/gold.rs` | 107 | Gold standard mechanics |
| `utilities/resolution.rs` | 20 | Conflict resolution |
| `infrastructure/pricing.rs` | 22 | Infrastructure pricing |
| `politics/chaos_config.rs` | 4 | Chaos configuration |

**Total zero-coverage: 947 lines (1.4% of workspace)**

### 1.4 Critical Low-Coverage Modules (<20%)

| Module | Coverage | Lines (cov/total) |
|---|---|---|
| `economy/justice/prison_labor.rs` | 2.1% | 7/329 |
| `i18n/mod.rs` | 2.2% | 1/46 |
| `engine/diagnostic.rs` | 2.7% | 10/366 |
| `economy/begging.rs` | 2.8% | 4/143 |
| `ui/snapshot.rs` | 2.8% | 58/2079 |
| `utilities/waste.rs` | 3.3% | 2/60 |
| `politics/lobbying.rs` | 3.3% | 6/181 |
| `politics/conservation.rs` | 6.0% | 16/267 |
| `politics/funding.rs` | 7.1% | 1/14 |
| `securities/derivatives.rs` | 8.3% | 6/72 |
| `corporate/federation.rs` | 10.4% | 15/144 |
| `securities/ccp.rs` | 10.9% | 6/55 |
| `economy/real_estate.rs` | 12.7% | 8/63 |
| `politics/bill_lifecycle.rs` | 12.7% | 55/434 |
| `military/fleet.rs` | 14.5% | 12/83 |
| `government/kio.rs` | 14.6% | 6/41 |
| `economy/state_sector/fishing.rs` | 16.0% | 16/100 |
| `entities/legal_form.rs` | 16.0% | 29/181 |
| `securities/brokerage.rs` | 16.2% | 13/80 |
| `politics/espionage.rs` | 17.5% | 10/57 |

**Notable:** `engine/diagnostic.rs` (2.7%) is the diagnostic harness itself —
low coverage because `CapturingProbe` is feature-gated and only exercised by
`phase94_diagnostic_harness_test.rs` under `--features diagnostic`.

### 1.5 Highest-Coverage Modules (100%)

20 modules achieve 100% coverage, primarily data registries and config:
`production_methods_data.rs` (5996 lines), `tech_tree_data.rs` (2095),
`crop_registry.rs` (449), `consumption_registry.rs` (266), etc.

### 1.6 Largest Epic/Diagnostic Test Files

| File | Lines | Tests | Classification |
|---|---|---|---|
| `waste_epic_test.rs` | 1,707 | 144 | Epic (stable mechanics) |
| `sanitation_epic_test.rs` | 1,212 | 89 | Epic (stable mechanics) |
| `thermal_epic_test.rs` | 897 | 70 | Epic (stable mechanics) |
| `supply_chain_integrity_test.rs` | 593 | 17 | Diagnostic |
| `ai_stability_audit_test.rs` | 529 | 17 | Diagnostic |
| `phase70_military_test.rs` | — | 18 | Phase integration |
| `world_gen_audit_test.rs` | 416 | 14 | Diagnostic |
| `phase94_diagnostic_harness_test.rs` | 351 | 1 | Feature-gated diagnostic |
| `macro_m0_audit.rs` | 321 | — | Critical M0 conservation |
| `banking_integration_test.rs` | 370 | — | Banking integration |

**Epic test subtotal:** 3,816 lines / 303 tests in 3 files (waste + sanitation +
thermal). These are the primary CI/CD bottleneck.

---

## 2. Snapshot Testing (cargo-insta)

### 2.1 Motivation

The codebase has 1,647 `assert_eq!` calls. Many verify complex struct outputs
that are brittle to cosmetic changes:

```rust
// CURRENT — brittle: breaks on field reorder, float format, HashMap order
assert_eq!(company.balance_sheet.total_assets(), 1_234_567.89);
assert_eq!(bank.loans_issued.len(), 5);
assert_eq!(format!("{:?}", market.base_prices), expected_string);
```

Snapshot testing captures the full serialized output once, stores it in a
`.snap` file, and compares future runs against the stored snapshot. When the
output legitimately changes, the developer reviews and approves the new
snapshot via `cargo insta review`.

### 2.2 Dependency

Add to `state/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3.10"
insta = { version = "1.40", features = ["ron"] }  # RON for readable diffs
```

**Why RON format?** Rust Object Notation produces human-readable snapshot
files with proper struct formatting, making diffs far more reviewable than
JSON for nested economic state (HashMaps, nested structs, f64 values).

### 2.3 Snapshot File Layout

```
state/src/snapshots/
├── economy/
│   ├── market__clearing_prices.snap
│   ├── banking__balance_sheet_after_turn.snap
│   └── ...
├── society/
│   ├── cadastre__land_allocation.snap
│   └── ...
└── state/
    ├── tax__revenue_breakdown.snap
    └── ...
```

Snapshots are **committed to git** (they are the source of truth for
mechanics validation). The `.snap.new` files (pending review) are
gitignored.

### 2.4 Migration Strategy

**No `assert_eq!` calls are deleted.** The migration is incremental:

1. **Phase 4a:** Add `insta` dev-dependency. No existing tests change.
2. **Phase 4b:** New tests for complex state outputs use `insta::assert_snapshot!`
   instead of `assert_eq!`.
3. **Phase 4c:** Existing `assert_eq!` blocks on complex outputs are converted
   to snapshots one module at a time, with auditor review of each `.snap` file.
4. **Phase 4d:** Simple boolean/numeric assertions (`assert!(x > 0.0)`,
   `assert_eq!(len, 5)`) remain as-is — snapshots are for complex state only.

**Conversion criteria** (when to use snapshot vs. assert):
- **Snapshot:** Struct serialization with >4 fields, HashMap output, float
  matrices, market clearing results, balance sheets, turn traces.
- **Keep assert:** Single boolean, single integer count, single float
  comparison, error condition checks.

### 2.5 Snapshot Test Pattern

```rust
use insta::assert_ron_snapshot;

#[test]
fn test_market_clearing_after_6_turns() {
    let (state, market, ...) = setup_6_turn_world();
    let result = serialize_clearing_state(&market, &state);

    // Snapshot captures the full RON-serialized state.
    // First run creates .snap.new; reviewer approves via cargo insta review.
    assert_ron_snapshot!("market__clearing_6_turns", result);
}
```

### 2.6 Float Handling

insta supports redactions for volatile values (timestamps, random IDs).
For float precision, use a redaction filter:

```rust
insta::with_settings!({
    redactions => vec![
        insta::dynamic_redaction("|value: &str| {
            // Round floats to 4 decimal places for stable snapshots
            // ...
        }"),
    ],
}, {
    assert_ron_snapshot!("banking__balance_sheet", result);
});
```

Alternatively, serialize with `serde_json` and use insta's JSON snapshot
with `{{float}}` redaction markers for volatile numeric fields.

---

## 3. `$approve_snapshots` Console Command

### 3.1 Purpose

`$approve_snapshots` provides a safe, guided workflow for reviewing snapshot
changes after a mechanics change. It wraps `cargo insta review` with
pre-flight checks and event emission.

### 3.2 Command Interface

```bash
bash .devin/scripts/console.sh $approve_snapshots [filter]
```

- `filter` (optional): substring filter for snapshot names (e.g.,
  `market` to review only market-related snapshots).

### 3.3 Implementation: `cmd_approve_snapshots()`

Add to `console.sh`:

```bash
cmd_approve_snapshots() {
    local filter="${1:-}"

    # 1. Pre-flight: verify clean working tree (no uncommitted source changes)
    if ! git diff --quiet -- state/src/ state/tests/; then
        echo "ERROR: Uncommitted source changes detected."
        echo "       Commit or stash before reviewing snapshots."
        echo ""
        git status --short -- state/src/ state/tests/
        exit 1
    fi

    # 2. Pre-flight: verify insta is installed
    if ! command -v cargo-insta &>/dev/null; then
        echo "ERROR: cargo-insta not installed."
        echo "       Run: cargo install cargo-insta --locked"
        exit 1
    fi

    # 3. Check for pending snapshots
    local pending
    pending=$(find state/src state/tests -name '*.snap.new' 2>/dev/null | wc -l)
    if [ "$pending" -eq 0 ]; then
        echo "No pending snapshots to review."
        exit 0
    fi
    echo "Found $pending pending snapshot(s) to review."

    # 4. Run cargo insta review with optional filter
    echo ""
    echo "Starting interactive snapshot review..."
    echo "  (accept: <a>, reject: <r>, skip: <s>)"
    echo ""
    if [ -n "$filter" ]; then
        cargo insta review --accept-unseen -- "$filter"
    else
        cargo insta review --accept-unseen
    fi
    local review_rc=$?

    # 5. Post-review: report results
    local remaining
    remaining=$(find state/src state/tests -name '*.snap.new' 2>/dev/null | wc -l)
    local approved=$((pending - remaining))

    echo ""
    echo "Snapshot review complete:"
    echo "  Approved: $approved"
    echo "  Rejected/Skipped: $remaining"

    # 6. Emit event for audit trail
    if [ "$approved" -gt 0 ]; then
        bash .devin/scripts/emit_event.sh \
            "SNAPSHOTS_APPROVED" \
            "agent-5" \
            "all" \
            "Approved $approved snapshot(s) after mechanics change review." \
            2>/dev/null || true
    fi

    # 7. Remind to commit
    if [ "$approved" -gt 0 ]; then
        echo ""
        echo "NOTE: Approved snapshots are in state/src/snapshots/."
        echo "      Commit them with: git add state/src/snapshots/ && git commit"
    fi

    exit $review_rc
}
```

### 3.4 Router Entry

Add to the `case` block in `console.sh`:

```bash
\$approve_snapshots) cmd_approve_snapshots "$@" ;;
```

### 3.5 Help Entry

Add to `cmd_help()`:

```
$approve_snapshots [filter]  - Review and approve pending cargo-insta snapshots
                              Pre-flight: clean tree check, insta installed
                              Post-review: emits SNAPSHOTS_APPROVED event
```

### 3.6 Safety Properties

1. **Clean-tree guard:** Prevents reviewing snapshots when uncommitted source
   changes exist (avoids approving snapshots for code that hasn't been
   committed).
2. **Insta presence check:** Fails fast with install instructions.
3. **No-pending fast-exit:** Exits 0 if no `.snap.new` files exist.
4. **Audit trail:** Emits `SNAPSHOTS_APPROVED` event for Agent 4 traceability.
5. **Commit reminder:** Does NOT auto-commit — the developer must explicitly
   commit approved snapshots.

---

## 4. Test Restructuring: Epic Segregation

### 4.1 Problem

The fast CI/CD pipeline (`integration_daemon.sh`) runs all 2,199 tests with
`cargo nextest run --workspace --all-targets`. The 303 epic tests in 3 files
(waste, sanitation, thermal) test stable mechanics that rarely change but
consume disproportionate CI time.

### 4.2 Solution: `tests/epics/` Directory

Create a new directory:

```
state/tests/
├── epics/                          # NEW: slow, comprehensive epic tests
│   ├── mod.rs                      # Module root (empty or re-exports)
│   ├── waste_epic_test.rs          # MOVED from tests/waste_epic_test.rs
│   ├── sanitation_epic_test.rs     # MOVED from tests/sanitation_epic_test.rs
│   ├── thermal_epic_test.rs        # MOVED from tests/thermal_epic_test.rs
│   ├── macro_m0_audit.rs           # MOVED — critical M0 conservation
│   ├── phase94_diagnostic_harness_test.rs  # MOVED — feature-gated diagnostic
│   ├── banking_integration_test.rs # MOVED — banking integration
│   ├── supply_chain_integrity_test.rs     # MOVED — supply chain
│   ├── world_gen_audit_test.rs     # MOVED — world gen audit
│   ├── ai_stability_audit_test.rs  # MOVED — AI stability
│   └── accounting_invariants_test.rs      # MOVED — double-entry invariants
│
├── headless_smoke_test.rs          # STAYS — fast smoke test
├── phase67_treaties_test.rs        # STAYS — fast phase tests
├── phase68_organizations_test.rs   # STAYS
├── ... (all fast unit/phase tests stay in tests/)
```

### 4.3 Classification Criteria

| Category | Location | CI Profile | Run When |
|---|---|---|---|
| **Fast unit/phase tests** | `state/tests/` | `fast` | Every CI run |
| **Epic/diagnostic tests** | `state/tests/epics/` | `epic` | Pre-merge, nightly, `$audit_standard` |
| **Feature-gated diagnostic** | `state/tests/epics/` | `epic` | Only with `--features diagnostic` |
| **Smoke test** | `state/tests/` | `smoke` | Every CI run (after fast) |

**Epic classification rule:** A test file is "epic" if it:
- Has >100 test functions, OR
- Runs a full multi-turn simulation (>6 turns), OR
- Tests cross-system conservation laws (M0, mass, balance-sheet), OR
- Is feature-gated with `#[cfg(feature = "diagnostic")]`

### 4.4 Cargo Test Targets and Feature Configuration

**v4.0.1 Patch:** The original design used `#[cfg(feature = "epic-tests")]`
inside each `.rs` file. This is **wrong** — Cargo still discovers the test
file and compiles an empty test binary, which confuses nextest (it sees
a binary with 0 tests and may report spurious failures or waste time on
test-binary setup). The correct approach is to use explicit `[[test]]`
blocks in `state/Cargo.toml` with `required-features`.

#### 4.4.1 Feature Flag

Add to `state/Cargo.toml`:

```toml
[features]
default = []
diagnostic = []
epic-tests = []  # NEW: gate epic test compilation
```

#### 4.4.2 Explicit `[[test]]` Blocks

For each epic test file, add a `[[test]]` block in `state/Cargo.toml`.
This tells Cargo exactly which files are test binaries and which features
they require. When `epic-tests` is not enabled, Cargo does not even
discover these files — no empty binaries, no nextest confusion.

```toml
# ============================================================================
# v4: Epic test targets — only compiled with --features epic-tests
# ============================================================================

[[test]]
name = "waste_epic_test"
path = "tests/epics/waste_epic_test.rs"
required-features = ["epic-tests"]

[[test]]
name = "sanitation_epic_test"
path = "tests/epics/sanitation_epic_test.rs"
required-features = ["epic-tests"]

[[test]]
name = "thermal_epic_test"
path = "tests/epics/thermal_epic_test.rs"
required-features = ["epic-tests"]

[[test]]
name = "macro_m0_audit"
path = "tests/epics/macro_m0_audit.rs"
required-features = ["epic-tests"]

[[test]]
name = "phase94_diagnostic_harness_test"
path = "tests/epics/phase94_diagnostic_harness_test.rs"
required-features = ["epic-tests", "diagnostic"]

[[test]]
name = "banking_integration_test"
path = "tests/epics/banking_integration_test.rs"
required-features = ["epic-tests"]

[[test]]
name = "supply_chain_integrity_test"
path = "tests/epics/supply_chain_integrity_test.rs"
required-features = ["epic-tests"]

[[test]]
name = "world_gen_audit_test"
path = "tests/epics/world_gen_audit_test.rs"
required-features = ["epic-tests"]

[[test]]
name = "ai_stability_audit_test"
path = "tests/epics/ai_stability_audit_test.rs"
required-features = ["epic-tests"]

[[test]]
name = "accounting_invariants_test"
path = "tests/epics/accounting_invariants_test.rs"
required-features = ["epic-tests"]
```

**Key points:**
- `required-features = ["epic-tests"]` — Cargo skips this test binary entirely
  when the feature is not enabled. No empty binary, no nextest confusion.
- `phase94_diagnostic_harness_test` requires BOTH `epic-tests` AND `diagnostic`
  because it uses `CapturingProbe` which is feature-gated.
- The `name` field controls the test binary name (e.g., `waste_epic_test`),
  keeping it identical to the current name for backwards compatibility.
- The `path` field points to the new location in `tests/epics/`.

#### 4.4.3 No `#[cfg]` Inside Test Files

**Do NOT add `#[cfg(feature = "epic-tests")]` to the top of each `.rs` file.**
The `required-features` in the `[[test]]` block handles compilation gating.
Adding `#[cfg]` inside the file would create an empty binary when the feature
is disabled — exactly the problem we are avoiding.

The only exception is `phase94_diagnostic_harness_test.rs`, which already has
`#![cfg(feature = "diagnostic")]` at the crate level (line 23). This is
redundant with the `required-features = ["epic-tests", "diagnostic"]` in the
`[[test]]` block, but is kept for backwards compatibility with any external
tooling that may run the test directly. The `[[test]]` block is the
authoritative gate.

#### 4.4.4 Nextest Profile Configuration

Update `.config/nextest.toml`:

```toml
# Architecture v4: Test segregation via Cargo features
# Epic tests are excluded from fast runs by NOT passing --features epic-tests.
# No nextest-level filter is needed — Cargo handles binary discovery.

[profile.ci]
retries = 2
fail-fast = false
status-level = "fail"
final-status-level = "fail"
```

### 4.5 CI/CD Pipeline Changes

Update `integration_daemon.sh` Stage 2b:

```bash
# --- Fast tests (every CI run) ---
# v4: Exclude epic tests — no --features epic-tests flag.
# v4.0.1: Export CI=true so cargo-insta fails hard on snapshot mismatches
#         instead of silently writing .snap.new files or hanging on prompts.
export CI=true
if command -v cargo-nextest &>/dev/null; then
    timeout 300 cargo nextest run --workspace --all-targets \
        --profile ci --test-threads=4 2>&1 | tee "${log_prefix}_test.txt" | tail -n 50
    # epic-tests feature NOT passed → [[test]] blocks with required-features
    # are skipped entirely by Cargo (no empty binaries, no nextest confusion)
else
    timeout 300 cargo test --workspace --all-targets -- --skip headless_50_tick_smoke 2>&1 | ...
fi

# --- Epic tests (pre-merge or $audit_standard only) ---
if [ "$RUN_EPIC_TESTS" = "1" ]; then
    echo "[$(date -u +%H:%M:%S)] CI/CD: [2b-epic] Running epic test suite..."
    # CI=true already exported above — insta strict mode is active.
    timeout 600 cargo nextest run --workspace --all-targets \
        --features epic-tests,diagnostic \
        --profile ci --test-threads=4 2>&1 | tee "${log_prefix}_epic_test.txt" | tail -n 50
    local epic_rc=${PIPESTATUS[0]}

    # v4.0.1 Patch: Artifact routing — extract absolute path to diagnostic_output/
    # and embed it in the AUDIT_REQUESTED event payload for Agent 4.
    if [ "$epic_rc" -eq 0 ]; then
        DIAG_OUTPUT_DIR="$(cd "$HUB_DIR/state/tests/diagnostic_output" 2>/dev/null && pwd)"
        if [ -n "$DIAG_OUTPUT_DIR" ] && [ -d "$DIAG_OUTPUT_DIR" ]; then
            echo "[$(date -u +%H:%M:%S)] Diagnostic artifacts at: $DIAG_OUTPUT_DIR"
            # Stash for AUDIT_REQUESTED emission (see §6.1)
            EPIC_DIAG_OUTPUT_PATH="$DIAG_OUTPUT_DIR"
        else
            echo "[$(date -u +%H:%M:%S)] WARNING: diagnostic_output/ not found — epic tests may not have produced dumps."
            EPIC_DIAG_OUTPUT_PATH=""
        fi
    fi
fi
```

**`CI=true` rationale:** `cargo-insta` checks the `CI` environment variable.
When set to `true`, insta:
- Fails the test immediately on snapshot mismatch (exit code != 0).
- Does NOT write `.snap.new` files (no silent drift).
- Does NOT hang waiting for interactive review input.

Without `CI=true`, a snapshot mismatch in CI would either silently write a
`.snap.new` file (which the daemon would never review) or hang indefinitely
waiting for stdin input that never comes, causing a timeout.

`RUN_EPIC_TESTS=1` is set by:
- `$audit_standard` command (full audit)
- Pre-merge gate (manager-triggered)
- Nightly cron (future)

### 4.6 Migration Steps

1. Add `epic-tests` feature to `state/Cargo.toml` `[features]` section.
2. Create `state/tests/epics/` directory.
3. Add 10 `[[test]]` blocks to `state/Cargo.toml` with `required-features`
   (see §4.4.2). **Do NOT add `#[cfg(feature)]` inside the `.rs` files.**
4. Move the 10 identified epic test files into `tests/epics/`.
5. Update `integration_daemon.sh`: add `export CI=true` before nextest,
   add `RUN_EPIC_TESTS` logic, add diagnostic_output path extraction
   (see §4.5).
6. Update `.config/nextest.toml` with profile documentation.
7. Verify fast CI still passes (epics excluded — Cargo skips `[[test]]`
   blocks whose `required-features` are not enabled).
8. Verify epic CI passes with `--features epic-tests,diagnostic`.
9. Verify `CI=true` causes insta to fail hard on snapshot mismatch (test
   by temporarily breaking a snapshot and confirming non-zero exit).

**No tests are deleted.** All 2,199 tests remain; they are just segregated
into fast and slow execution tiers.

---

## 5. Auditor Data Analytics: JSON Ledger Dumps

### 5.1 Motivation

Currently, the M0 conservation test (`macro_m0_audit.rs`) and the diagnostic
harness (`phase94_diagnostic_harness_test.rs`) verify double-entry invariants
inside Rust. The auditor (Agent 4) sees only pass/fail. To independently
verify the math, the auditor must read Rust source code — which is
impractical for a Python-based auditor agent.

v4 introduces **structured JSON ledger dumps** that the Rust test harness
emits after a diagnostic run. Agent 4 parses these with external Python
scripts to mathematically verify invariants without touching Rust.

### 5.2 Existing Infrastructure

The codebase already has a JSON dump mechanism in
`state/src/engine/diagnostic.rs`:

- `TurnTrace` (serializable struct with `harness_version`, `targets`,
  `turns: Vec<TurnRecord>`, `summary: TraceSummary`)
- `PhaseCheckpoint` (per-phase snapshot with `global_fiat: FiatWalk`,
  `global_mass`, `companies`, `bank`, `regional_market`, `conservation`,
  `loan_events`)
- `write_turn_trace_json()` — writes `TurnTrace` to `.json`
- `write_turn_summary_csv()` — writes flat CSV summary
- Output directory: `state/tests/diagnostic_output/`

v4 extends this with **sector-specific ledger dumps** that are independent
of the `TurnTrace` structure.

### 5.3 New JSON Dump Schemas

#### 5.3.1 Sector Ledger Dump

```json
{
  "dump_version": "v4.0",
  "dump_type": "sector_ledger",
  "turn": 6,
  "year": 1951,
  "country": "Poland",
  "sectors": {
    "Agriculture": {
      "companies": [
        {
          "id": "company_ag_001",
          "liquid_capital": 45000.0,
          "available_cash": 12000.0,
          "debit_cash": 3000.0,
          "credit_cash": 8000.0,
          "liabilities": 25000.0,
          "fixed_capital": 180000.0,
          "revenue_turn": 15000.0,
          "opex_turn": 9000.0,
          "wages_paid_turn": 4000.0,
          "taxes_paid_turn": 750.0,
          "dividends_paid_turn": 0.0,
          "inventory_value": 22000.0
        }
      ],
      "sector_totals": {
        "liquid_capital": 45000.0,
        "available_cash": 12000.0,
        "revenue": 15000.0,
        "opex": 9000.0,
        "wages": 4000.0,
        "taxes": 750.0
      }
    },
    "HeavyIndustry": { ... },
    "Banking": { ... }
  },
  "cross_sector_flows": {
    "b2b_payments": 45000.0,
    "inter_sector_transfers": [
      {"from": "Agriculture", "to": "HeavyIndustry", "amount": 12000.0, "commodity": "Food"}
    ]
  }
}
```

#### 5.3.2 Market Clearing Loop Dump

```json
{
  "dump_version": "v4.0",
  "dump_type": "market_clearing",
  "turn": 6,
  "year": 1951,
  "iterations": [
    {
      "iteration": 0,
      "commodity": "Steel",
      "supply_volume": 5000.0,
      "demand_volume": 4800.0,
      "clearing_price": 12.50,
      "net_surplus": 200.0,
      "unfilled_bids": 0.0,
      "unfilled_asks": 200.0
    },
    {
      "iteration": 1,
      "commodity": "Steel",
      "supply_volume": 5000.0,
      "demand_volume": 4900.0,
      "clearing_price": 12.75,
      "net_surplus": 100.0,
      "unfilled_bids": 0.0,
      "unfilled_asks": 100.0
    }
  ],
  "final_prices": {
    "Steel": 12.75,
    "Food": 3.20,
    "Energy": 8.40
  },
  "convergence": {
    "converged": true,
    "iterations_to_converge": 3,
    "max_price_delta_final": 0.01
  }
}
```

#### 5.3.3 Commercial Banking State Dump

```json
{
  "dump_version": "v4.0",
  "dump_type": "commercial_banking",
  "turn": 6,
  "year": 1951,
  "banks": [
    {
      "id": "bank_001",
      "name": "Bank Przemyslowy",
      "balance_sheet": {
        "assets": {
          "reserves_at_central_bank": 50000.0,
          "cb_deposit_facility_balance": 10000.0,
          "loans_issued": 200000.0,
          "securities": 30000.0,
          "interbank_loans_given": 5000.0
        },
        "liabilities": {
          "deposits": 240000.0,
          "cb_lombard_loans": 15000.0,
          "interbank_loans_taken": 8000.0,
          "tier_1_capital": 32000.0
        },
        "equity": 32000.0,
        "total_assets": 295000.0,
        "total_liabilities": 263000.0,
        "is_balanced": true,
        "balance_check": "assets == liabilities + equity",
        "balance_delta": 0.0
      },
      "loans": [
        {
          "id": "loan_001",
          "borrower_id": "company_ag_001",
          "principal": 50000.0,
          "outstanding_balance": 42000.0,
          "interest_rate": 0.08,
          "turns_remaining": 12,
          "status": "Active"
        }
      ]
    }
  ],
  "banking_system_totals": {
    "total_reserves": 60000.0,
    "total_deposits": 240000.0,
    "total_loans_outstanding": 200000.0,
    "total_interbank_exposure": 13000.0,
    "money_multiplier_estimate": 4.0
  },
  "central_bank": {
    "reserves_held": 60000.0,
    "lombard_loans_outstanding": 15000.0,
    "liquidity_injected_cumulative": 75000.0,
    "deposit_facility_balance": 10000.0
  }
}
```

### 5.4 Rust Implementation: Dump Functions

Add to `state/src/engine/diagnostic.rs` (feature-gated):

```rust
// ============================================================================
// v4: SECTOR LEDGER DUMPS (for external Python auditor verification)
// ============================================================================

/// Sector ledger dump — one sector's company financials + cross-sector flows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorLedgerDump {
    pub dump_version: String,
    pub dump_type: String,
    pub turn: u32,
    pub year: u32,
    pub country: String,
    pub sectors: HashMap<String, SectorEntry>,
    pub cross_sector_flows: CrossSectorFlows,
}

/// Market clearing loop dump — per-iteration state for convergence analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketClearingDump {
    pub dump_version: String,
    pub dump_type: String,
    pub turn: u32,
    pub year: u32,
    pub iterations: Vec<ClearingIteration>,
    pub final_prices: HashMap<Commodity, f64>,
    pub convergence: ConvergenceInfo,
}

/// Commercial banking state dump — full bank balance sheets + loan books.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankingStateDump {
    pub dump_version: String,
    pub dump_type: String,
    pub turn: u32,
    pub year: u32,
    pub banks: Vec<BankDumpEntry>,
    pub banking_system_totals: BankingSystemTotals,
    pub central_bank: CentralBankDump,
}

// ... (field structs omitted for brevity — see schemas above)

/// Write all three dump types to a directory.
#[cfg(feature = "diagnostic")]
pub fn write_all_dumps(
    state: &GameState,
    market: &GlobalMarket,
    tasks: &[CountryTask<'_>],
    turn: u32,
    year: u32,
    output_dir: &Path,
) -> std::io::Result<()> {
    let sector = build_sector_ledger_dump(state, tasks, turn, year);
    let clearing = build_market_clearing_dump(market, turn, year);
    let banking = build_banking_state_dump(state, tasks, turn, year);

    std::fs::write(output_dir.join("sector_ledger.json"),
        serde_json::to_string_pretty(&sector)?)?;
    std::fs::write(output_dir.join("market_clearing.json"),
        serde_json::to_string_pretty(&clearing)?)?;
    std::fs::write(output_dir.join("banking_state.json"),
        serde_json::to_string_pretty(&banking)?)?;

    Ok(())
}
```

### 5.5 Output Directory Structure

```
state/tests/diagnostic_output/
├── turn_trace_q1.json              # EXISTING — full TurnTrace
├── turn_summary_q1.csv             # EXISTING — flat CSV
├── sector_ledger.json              # NEW — sector financials
├── market_clearing.json            # NEW — clearing loop state
├── banking_state.json              # NEW — commercial banking
└── manifest.json                   # NEW — dump metadata
```

`manifest.json`:
```json
{
  "dump_version": "v4.0",
  "generated_at": "2026-09-07T12:00:00Z",
  "engine_version": "1.2.0",
  "turns_run": 6,
  "features_enabled": ["diagnostic"],
  "files": ["sector_ledger.json", "market_clearing.json", "banking_state.json"]
}
```

### 5.6 Auditor (Agent 4) Python Verification

**The Python scripts are NOT written in this phase.** This section
specifies the interface they will consume.

#### 5.6.1 Double-Entry Invariant Verification

Agent 4's Python script will verify:

1. **Bank balance-sheet identity:** For each bank in `banking_state.json`:
   ```
   total_assets == total_liabilities + equity
   balance_delta == 0.0 (within tolerance 1e-6)
   ```

2. **M0 conservation:** Using `banking_state.json` + `sector_ledger.json`:
   ```
   M0 = treasury_cash + citizen_cash + bank_reserves + offshore_capital
        + see_charity_pool + ministry_cash
   ΔM0 == Δcb_injection + Δtreasury_external_financing
   ```

3. **Sector flow conservation:** Using `sector_ledger.json`:
   ```
   For each sector: revenue == opex + wages + taxes + dividends + net_inventory_change
   Cross-sector: sum(b2b_payments) == sum(inter_sector_transfers.amount)
   ```

4. **Market clearing convergence:** Using `market_clearing.json`:
   ```
   For each commodity: |supply - demand| <= convergence_threshold
   unfilled_bids + unfilled_asks <= tolerance
   ```

#### 5.6.2 Python Script Interface

```python
# .devin/scripts/audit_dumps.py (NOT YET WRITTEN)
#
# Usage: python .devin/scripts/audit_dumps.py <dump_dir>
#
# Reads:  <dump_dir>/sector_ledger.json
#         <dump_dir>/market_clearing.json
#         <dump_dir>/banking_state.json
#
# Outputs: PASS/FAIL per invariant + detailed violation report
#
# The script does NOT import any Rust code. It operates purely on the
# JSON schemas defined in §5.3. This allows the auditor to verify
# invariants without a Rust toolchain.
```

#### 5.6.3 Decoupling Principle

The JSON dumps are the **sole interface** between the Rust engine and the
Python auditor. This ensures:

- The auditor can run without a Rust toolchain (only Python + the JSON files).
- The JSON schema is versioned (`dump_version`) for forward compatibility.
- The auditor can verify invariants that the Rust tests assert, providing
  independent cross-validation.
- New invariants can be added by extending the JSON schema + Python script,
  without modifying Rust test code.

---

## 6. CI/CD Pipeline Integration Summary

### 6.1 Modified Flow

```
PROMOTED_TO_MAIN
      |
      v
Agent 4 auto_wake.sh
      |
      v
Audit execution
      |
      +--> AUDIT_PASS
      |
      +--> AUDIT_FAIL / AUDIT_FAIL_ADDENDUM
                  |
                  v
      Parse blueprint_agent_map.json
                  |
                  v
      Emit REMEDIATION_REQUESTED
                  |
                  v
Worker auto_wake.sh
      |
      v
Local main synchronization
      |
      v
cargo nextest run --workspace           # v4: FAST (no epics, CI=true)
      |
      v
INTEGRATION_REQUESTED
      |
      v
Daemon staging pipeline
      |
      +--> [2a] cargo test --no-run
      +--> [2b-fast] export CI=true; cargo nextest (no epic-tests)  # v4
      +--> [2b-epic] cargo nextest --features epic-tests,diagnostic # v4 (pre-merge)
      |                |
      |                v
      |        Extract diagnostic_output/ absolute path    # v4.0.1
      |                |
      |                v
      |        Embed path in AUDIT_REQUESTED payload       # v4.0.1
      |
      +--> [2b-doc] cargo test --doc
      +--> [3] cargo clippy
      +--> [4] npm run build
      +--> [5] headless smoke test
      |
      v
Promotion and subsequent audit
```

#### 6.1.1 AUDIT_REQUESTED Event Payload (v4.0.1 Patch)

The existing `AUDIT_REQUESTED` event in `integration_daemon.sh` (line ~728)
currently sends:

```json
{"branch":"$EVENT_BRANCH","staging_commit":"$staging_commit"}
```

**v4.0.1 patches this to include the diagnostic output path:**

```bash
# v4.0.1: Embed diagnostic_output absolute path in AUDIT_REQUESTED payload
# so Agent 4 can locate JSON dumps for external Python validation.
local audit_payload
if [ -n "${EPIC_DIAG_OUTPUT_PATH:-}" ]; then
    audit_payload="{\"branch\":\"$EVENT_BRANCH\",\"staging_commit\":\"$staging_commit\",\"diagnostic_output_path\":\"$EPIC_DIAG_OUTPUT_PATH\"}"
else
    audit_payload="{\"branch\":\"$EVENT_BRANCH\",\"staging_commit\":\"$staging_commit\"}"
fi

bash "$SCRIPT_DIR/emit_event.sh" "AUDIT_REQUESTED" "agent-5" "agent-4" \
    "$audit_payload" 2>/dev/null
```

**The same patch applies to the sprint-completion AUDIT_REQUESTED emission**
(line ~1122):

```bash
local sprint_audit_payload
if [ -n "${EPIC_DIAG_OUTPUT_PATH:-}" ]; then
    sprint_audit_payload="{\"reason\":\"All sprint manifest branches merged to main. Run comprehensive 23-rule macro-architectural audit.\",\"audit_type\":\"full_macro_architectural\",\"diagnostic_output_path\":\"$EPIC_DIAG_OUTPUT_PATH\"}"
else
    sprint_audit_payload='{"reason":"All sprint manifest branches merged to main. Run comprehensive 23-rule macro-architectural audit.","audit_type":"full_macro_architectural"}'
fi

bash "$SCRIPT_DIR/emit_event.sh" "AUDIT_REQUESTED" "agent-5" "agent-4" \
    "$sprint_audit_payload" 2>/dev/null
```

**Agent 4 consumption:** The auditor's `auto_wake.sh` parses the event
payload and extracts `diagnostic_output_path`. If present, Agent 4 runs:

```bash
python .devin/scripts/audit_dumps.py "$diagnostic_output_path"
```

If `diagnostic_output_path` is absent or empty (epic tests were not run),
Agent 4 proceeds with the standard audit workflow without Python dump
verification.

### 6.2 Trigger Matrix

| Trigger | Fast Tests | Epic Tests | Smoke | Clippy | npm | CI=true | Diag Path in AUDIT_REQUESTED |
|---|---|---|---|---|---|---|---|
| Worker integration request | Yes | No | Yes | Yes | Yes | Yes | No (epics not run) |
| `$audit_standard` | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Pre-merge gate (manager) | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Nightly (future) | Yes | Yes | Yes | Yes | Yes | Yes | Yes |

---

## 7. Dependency Installation

### 7.1 cargo-tarpaulin (Already Installed)

```bash
cargo install cargo-tarpaulin --locked
# Installed: cargo-tarpaulin v0.37.2
```

### 7.2 cargo-insta (To Be Installed)

```bash
cargo install cargo-insta --locked
```

### 7.3 insta crate (dev-dependency)

Add to `state/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3.10"
insta = { version = "1.40", features = ["ron"] }
```

---

## 8. Implementation Sequence

### Phase 4a: Foundation (no behavior change)
1. Install `cargo-insta`.
2. Add `insta` dev-dependency to `state/Cargo.toml`.
3. Add `epic-tests` feature to `state/Cargo.toml` `[features]` section.
4. Create `state/tests/epics/` directory.
5. Update `.gitignore` for `*.snap.new` files.

### Phase 4b: Test Segregation (v4.0.1 patched)
6. Add 10 `[[test]]` blocks to `state/Cargo.toml` with `required-features`
   (see §4.4.2). **Do NOT add `#[cfg(feature)]` inside `.rs` files.**
7. Move the 10 epic test files into `state/tests/epics/`.
8. Update `integration_daemon.sh`:
   - Add `export CI=true` before nextest (§4.5 — insta strict mode).
   - Add `RUN_EPIC_TESTS` logic for epic test execution.
   - Add diagnostic_output path extraction after epic tests (§4.5).
   - Patch AUDIT_REQUESTED payload to include `diagnostic_output_path`
     (§6.1.1).
9. Update `request_integration.sh` to run fast tests only (no `--features
   epic-tests`).
10. Verify fast CI passes (epics excluded — Cargo skips `[[test]]` blocks
    whose `required-features` are not enabled).
11. Verify epic CI passes with `--features epic-tests,diagnostic`.
12. Verify `CI=true` causes insta to fail hard on snapshot mismatch.

### Phase 4c: Snapshot Testing
13. Write first snapshot tests for market clearing output.
14. Write snapshot tests for bank balance sheet serialization.
15. Add `cmd_approve_snapshots()` to `console.sh`.
16. Add `$approve_snapshots` to router and help.
17. Update `COMMAND_REFERENCE.md` with `$approve_snapshots`.
18. Update `SOP.md` with snapshot review workflow.

### Phase 4d: Auditor Data Analytics
19. Implement `SectorLedgerDump`, `MarketClearingDump`, `BankingStateDump`
    structs in `diagnostic.rs`.
20. Implement `build_*_dump()` and `write_all_dumps()` functions.
21. Integrate dump calls into `phase94_diagnostic_harness_test.rs`.
22. Verify JSON files are emitted to `diagnostic_output/`.
23. Verify daemon extracts absolute path and embeds in AUDIT_REQUESTED.
24. Document JSON schemas in this file (already done in §5.3).
25. **Python scripts are deferred to a separate task.**

### Phase 4e: Documentation
26. Update `.devin/SOP.md` with v4 testing workflow.
27. Update `.devin/COMMAND_REFERENCE.md` with `$approve_snapshots`.
28. Update `AGENTS.md` with epic test feature flag documentation.
29. Update `.config/nextest.toml` with profile documentation.

---

## 9. Risk Analysis

| Risk | Mitigation |
|---|---|
| Snapshot files bloat the repo | Use RON format (compact), git LFS if needed |
| Snapshot review friction | `$approve_snapshots` provides guided workflow |
| **Insta hangs in CI on snapshot mismatch** | **`export CI=true` forces hard failure (v4.0.1 patch)** |
| **Empty test binaries confuse nextest** | **`[[test]]` blocks with `required-features` replace `#[cfg]` (v4.0.1 patch)** |
| **Agent 4 cannot find JSON dumps** | **`diagnostic_output_path` embedded in AUDIT_REQUESTED payload (v4.0.1 patch)** |
| Epic test move breaks imports | `[[test]]` blocks explicitly specify `path` — no auto-discovery ambiguity |
| Feature flag confusion | Document clearly; `epic-tests` is additive, not breaking |
| JSON schema drift | Versioned with `dump_version` field; Python checks version |
| Float precision in JSON | Use `serde_json::Number` with full f64 precision; Python uses `decimal.Decimal` |
| Tarpaulin Windows limitations | Results are line-coverage only (no branch); acceptable for macro analysis |
| `EPIC_DIAG_OUTPUT_PATH` stale across cycles | Daemon resets variable at start of each cycle; only set after successful epic run |

---

## 10. Success Criteria

- [ ] `cargo-insta` installed and `insta` dev-dependency added
- [ ] `epic-tests` feature flag added to `state/Cargo.toml`
- [ ] 10 `[[test]]` blocks added to `state/Cargo.toml` with `required-features`
- [ ] **No `#[cfg(feature)]` inside epic test `.rs` files** (v4.0.1)
- [ ] 10 epic test files moved to `state/tests/epics/`
- [ ] Fast CI runs without epic tests (Cargo skips `[[test]]` blocks)
- [ ] Epic CI runs with `--features epic-tests,diagnostic`
- [ ] **`export CI=true` before every `cargo nextest` invocation** (v4.0.1)
- [ ] **Insta fails hard on snapshot mismatch in CI** (no `.snap.new`, no hang)
- [ ] `$approve_snapshots` command functional in `console.sh`
- [ ] First snapshot tests for market clearing and banking
- [ ] JSON dump functions implemented in `diagnostic.rs`
- [ ] JSON dumps emitted by diagnostic harness test
- [ ] **`AUDIT_REQUESTED` event includes `diagnostic_output_path`** (v4.0.1)
- [ ] **Agent 4 can locate JSON dumps from event payload alone** (v4.0.1)
- [ ] Documentation updated (SOP, COMMAND_REFERENCE, AGENTS.md)
- [ ] No existing tests deleted
- [ ] Workspace coverage maintained or improved

---

## Appendix A: Tarpaulin Raw Output Summary

```
70.50% coverage, 47731/67702 lines covered

Zero-coverage (0/N):
  economy/pension.rs: 0/307
  infrastructure/effects.rs: 0/158
  infrastructure/pricing.rs: 0/22
  politics/chaos_config.rs: 0/4
  state/gold.rs: 0/107
  state/special_economic_zones.rs: 0/145
  utilities/demand.rs: 0/184
  utilities/resolution.rs: 0/20

Lowest non-zero (<20%):
  economy/justice/prison_labor.rs: 7/329 (2.1%)
  i18n/mod.rs: 1/46 (2.2%)
  engine/diagnostic.rs: 10/366 (2.7%)
  economy/begging.rs: 4/143 (2.8%)
  ui/snapshot.rs: 58/2079 (2.8%)
  utilities/waste.rs: 2/60 (3.3%)
  politics/lobbying.rs: 6/181 (3.3%)
  politics/conservation.rs: 16/267 (6.0%)
  politics/funding.rs: 1/14 (7.1%)
  securities/derivatives.rs: 6/72 (8.3%)
  corporate/federation.rs: 15/144 (10.4%)
  securities/ccp.rs: 6/55 (10.9%)
  economy/real_estate.rs: 8/63 (12.7%)
  politics/bill_lifecycle.rs: 55/434 (12.7%)
  military/fleet.rs: 12/83 (14.5%)
  government/kio.rs: 6/41 (14.6%)
  economy/state_sector/fishing.rs: 16/100 (16.0%)
  entities/legal_form.rs: 29/181 (16.0%)
  securities/brokerage.rs: 13/80 (16.2%)
  politics/espionage.rs: 10/57 (17.5%)

100% coverage (20 modules):
  registries/production_methods_data.rs: 5996/5996
  registries/tech_tree_data.rs: 2095/2095
  data/crop_registry.rs: 449/449
  data/consumption_registry.rs: 266/266
  military/config.rs: 118/118
  economy/indicators.rs: 109/109
  economy/labor/labor_config.rs: 100/100
  registries/blueprint_specs.rs: 75/75
  military/morale.rs: 69/69
  energy/municipal_heating_ai.rs: 62/62
  corporate/market_behavior.rs: 53/53
  registries/government.rs: 52/52
  economy/labor/disability_config.rs: 52/52
  politics/attendance.rs: 50/50
  economy/config/education_config.rs: 49/49
  military/propaganda.rs: 46/46
  economy/market/market_history.rs: 43/43
  environment/parks.rs: 41/41
  society/geography_config.rs: 40/40
  data/perishability_registry.rs: 39/39
```

---

## Appendix B: Epic Test File Inventory

| File | Lines | Tests | Action | Reason |
|---|---|---|---|---|
| `waste_epic_test.rs` | 1,707 | 144 | Move to `epics/` | 144 tests, stable mechanics |
| `sanitation_epic_test.rs` | 1,212 | 89 | Move to `epics/` | 89 tests, stable mechanics |
| `thermal_epic_test.rs` | 897 | 70 | Move to `epics/` | 70 tests, stable mechanics |
| `macro_m0_audit.rs` | 321 | — | Move to `epics/` | Multi-turn M0 conservation |
| `phase94_diagnostic_harness_test.rs` | 351 | 1 | Move to `epics/` | Feature-gated, 6-turn sim |
| `banking_integration_test.rs` | 370 | — | Move to `epics/` | Banking integration |
| `supply_chain_integrity_test.rs` | 593 | 17 | Move to `epics/` | Cross-system supply chain |
| `world_gen_audit_test.rs` | 416 | 14 | Move to `epics/` | World gen audit |
| `ai_stability_audit_test.rs` | 529 | 17 | Move to `epics/` | AI stability audit |
| `accounting_invariants_test.rs` | 224 | 9 | Move to `epics/` | Double-entry invariants |

**Total moved: 10 files, ~6,620 lines, ~360+ tests**

---

*End of Architecture v4 Testing Plan*
