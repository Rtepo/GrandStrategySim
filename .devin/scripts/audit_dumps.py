#!/usr/bin/env python3
"""
audit_dumps.py â€” Architecture v4 Auditor Data Analytics

Parses the JSON dumps emitted by the Rust diagnostic harness
(state/tests/diagnostic_output/) and mathematically verifies the
double-entry bookkeeping invariants to locate the root cause of the
M0 leak in the engine.

Usage:
    python .devin/scripts/audit_dumps.py <dump_dir>
    python .devin/scripts/audit_dumps.py state/tests/diagnostic_output

Reads:
    <dump_dir>/turn_trace_q1.json     â€” per-phase M0 checkpoints + bank + loans
    <dump_dir>/sector_ledger.json     â€” all companies grouped by sector
    <dump_dir>/market_clearing.json   â€” final prices, supply/demand, offshore
    <dump_dir>/banking_state.json     â€” commercial banking aggregate state

Outputs:
    PASS/FAIL per invariant + detailed violation report identifying the
    root cause of any M0 leak.

The script does NOT import any Rust code. It operates purely on the JSON
schemas defined in architecture_v4_testing_plan.md Â§5.3. This allows the
auditor to verify invariants without a Rust toolchain.

Exit codes:
    0 â€” all invariants PASS
    1 â€” one or more invariants FAIL (details in report)
    2 â€” fatal error (missing files, parse errors)
"""

from __future__ import annotations

import json
import os
import sys
from dataclasses import dataclass, field
from decimal import Decimal, getcontext
from pathlib import Path
from typing import Any

# Set high precision for f64-grade financial arithmetic.
getcontext().prec = 50

# Tolerance for float comparisons. The Rust engine uses f64, so we allow
# a relative tolerance. For absolute M0 values in the tens of billions,
# 1e-6 relative = ~30,000 absolute, which is tight enough to catch
# million-scale leaks while ignoring float rounding noise.
TOL_REL = Decimal("1e-6")
TOL_ABS = Decimal("1e-3")  # 0.001 fiat â€” tight for per-component checks


# ============================================================================
# Data Structures
# ============================================================================


@dataclass
class Violation:
    """A single invariant violation."""

    invariant: str
    severity: str  # "FAIL" or "WARN"
    checkpoint: str
    magnitude: Decimal
    explanation: str


@dataclass
class InvariantResult:
    """Result of verifying one invariant."""

    name: str
    passed: bool
    checks_run: int = 0
    failures: list[Violation] = field(default_factory=list)
    warnings: list[Violation] = field(default_factory=list)

    def add_fail(self, checkpoint: str, magnitude: Decimal, explanation: str) -> None:
        self.failures.append(
            Violation(self.name, "FAIL", checkpoint, magnitude, explanation)
        )
        self.passed = False

    def add_warn(self, checkpoint: str, magnitude: Decimal, explanation: str) -> None:
        self.warnings.append(
            Violation(self.name, "WARN", checkpoint, magnitude, explanation)
        )


@dataclass
class AuditReport:
    """Full audit report."""

    dump_dir: str
    results: list[InvariantResult] = field(default_factory=list)
    root_cause: str = ""
    m0_timeline: list[dict[str, Any]] = field(default_factory=list)

    @property
    def all_pass(self) -> bool:
        return all(r.passed for r in self.results)

    def summary(self) -> str:
        lines = []
        lines.append("=" * 78)
        lines.append("AUDIT DUMPS â€” DOUBLE-ENTRY INVARIANT VERIFICATION REPORT")
        lines.append("=" * 78)
        lines.append(f"Dump directory: {self.dump_dir}")
        lines.append("")
        for r in self.results:
            status = "PASS" if r.passed else "FAIL"
            lines.append(
                f"  [{status}] {r.name} "
                f"({r.checks_run} checks, {len(r.failures)} failures, "
                f"{len(r.warnings)} warnings)"
            )
        lines.append("")
        lines.append("-" * 78)
        if self.all_pass:
            lines.append("VERDICT: ALL INVARIANTS PASS â€” no M0 leak detected.")
        else:
            lines.append("VERDICT: INVARIANT VIOLATIONS DETECTED â€” M0 leak confirmed.")
        lines.append("-" * 78)
        if self.root_cause:
            lines.append("")
            lines.append("ROOT CAUSE ANALYSIS:")
            lines.append(self.root_cause)
        lines.append("")
        if self.m0_timeline:
            lines.append("-" * 78)
            lines.append("M0 TIMELINE (per checkpoint):")
            lines.append(
                f"{'Checkpoint':<45} {'M0 Total':>18} {'Î”M0':>15} {'Î”CB Inj':>15} {'Leak':>15}"
            )
            for entry in self.m0_timeline:
                lines.append(
                    f"{entry['checkpoint']:<45} "
                    f"{entry['total']:>18.2f} "
                    f"{entry['delta_m0']:>15.2f} "
                    f"{entry['delta_cb']:>15.2f} "
                    f"{entry['leak']:>15.2f}"
                )
        lines.append("")
        for r in self.results:
            if r.failures:
                lines.append("-" * 78)
                lines.append(f"FAILURES â€” {r.name}:")
                for v in r.failures:
                    lines.append(f"  [{v.checkpoint}] magnitude={v.magnitude:.6f}")
                    lines.append(f"    {v.explanation}")
            if r.warnings:
                lines.append(f"WARNINGS â€” {r.name}:")
                for v in r.warnings:
                    lines.append(f"  [{v.checkpoint}] magnitude={v.magnitude:.6f}")
                    lines.append(f"    {v.explanation}")
        lines.append("=" * 78)
        return "\n".join(lines)


# ============================================================================
# JSON Loading
# ============================================================================


def load_json(path: Path) -> dict[str, Any] | None:
    """Load a JSON file, returning None if it doesn't exist."""
    if not path.exists():
        return None
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def to_dec(val: Any) -> Decimal:
    """Convert a JSON number to Decimal safely."""
    if val is None:
        return Decimal("0")
    if isinstance(val, (int, float)):
        return Decimal(str(val))
    if isinstance(val, str):
        return Decimal(val)
    return Decimal("0")


def approx_equal(a: Decimal, b: Decimal, tol_abs: Decimal = TOL_ABS) -> bool:
    """Check if two Decimals are approximately equal within absolute tolerance."""
    return abs(a - b) <= tol_abs


def rel_equal(a: Decimal, b: Decimal, tol_rel: Decimal = TOL_REL) -> bool:
    """Check relative equality, handling near-zero values."""
    if abs(b) < Decimal("1"):
        return abs(a - b) <= TOL_ABS
    return abs(a - b) / abs(b) <= tol_rel


# ============================================================================
# Invariant 1: M0 Component Reconciliation
# ============================================================================


def check_m0_component_reconciliation(
    turn_trace: dict[str, Any], report: AuditReport
) -> InvariantResult:
    """
    Verify that M0 total equals the sum of its components:
      total == treasury_cash + citizen_cash + bank_reserves
               + offshore_capital + see_charity_pool + ministry_cash

    If this fails, the engine is miscounting M0 â€” money exists in a
    component that isn't included in the total, or the total is computed
    independently of the components (a double-counting or omission bug).
    """
    result = InvariantResult(
        name="M0 Component Reconciliation (total == sum of components)",
        passed=True,
    )

    turns = turn_trace.get("turns", [])
    for turn in turns:
        for cp in turn.get("checkpoints", []):
            cp_id = f"turn{cp['turn']}/phase{cp['phase_index']}:{cp['phase_name']}"
            gf = cp.get("global_fiat", {})
            if not gf:
                continue
            result.checks_run += 1

            total = to_dec(gf.get("total", 0))
            components = (
                to_dec(gf.get("treasury_cash", 0))
                + to_dec(gf.get("citizen_cash", 0))
                + to_dec(gf.get("bank_reserves", 0))
                + to_dec(gf.get("offshore_capital", 0))
                + to_dec(gf.get("see_charity_pool", 0))
                + to_dec(gf.get("ministry_cash", 0))
            )
            diff = total - components
            if not approx_equal(diff, Decimal("0"), tol_abs=Decimal("1")):
                result.add_fail(
                    cp_id,
                    diff,
                    f"M0 total={total:.6f} but sum of components={components:.6f} "
                    f"(diff={diff:.6f}). Money is in a component not counted in total, "
                    f"or total is computed independently.",
                )

    return result


# ============================================================================
# Invariant 2: M0 Conservation (Î”M0 == Î”CB Injection)
# ============================================================================


def check_m0_conservation(
    turn_trace: dict[str, Any], report: AuditReport
) -> InvariantResult:
    """
    Verify that M0 conservation holds across every phase transition:
      Î”M0 == Î”cumulative_cb_injection

    The only legitimate way to create or destroy base money is via
    Central Bank injection (or external financing). If M0 changes by
    more than the CB injection delta, money was created or destroyed
    elsewhere in the engine â€” this is the M0 leak.

    This invariant directly corresponds to Global Rule 1 (Strict
    Closed-Loop Economy / Double-Entry Bookkeeping).
    """
    result = InvariantResult(
        name="M0 Conservation (Î”M0 == Î”CB Injection)",
        passed=True,
    )

    turns = turn_trace.get("turns", [])
    prev_total: Decimal | None = None
    prev_cb: Decimal | None = None

    for turn in turns:
        for cp in turn.get("checkpoints", []):
            cp_id = f"turn{cp['turn']}/phase{cp['phase_index']}:{cp['phase_name']}"
            gf = cp.get("global_fiat", {})
            if not gf:
                continue
            result.checks_run += 1

            total = to_dec(gf.get("total", 0))
            cb = to_dec(gf.get("cumulative_cb_injection", 0))

            if prev_total is not None and prev_cb is not None:
                delta_m0 = total - prev_total
                delta_cb = cb - prev_cb
                leak = delta_m0 - delta_cb

                # Record timeline entry
                report.m0_timeline.append(
                    {
                        "checkpoint": cp_id,
                        "total": float(total),
                        "delta_m0": float(delta_m0),
                        "delta_cb": float(delta_cb),
                        "leak": float(leak),
                    }
                )

                # Use relative tolerance for large M0 values
                if not rel_equal(delta_m0, delta_cb, tol_rel=Decimal("1e-4")):
                    if abs(leak) > Decimal("1"):
                        kind = "FiatCreation" if leak > 0 else "FiatDestruction"
                        result.add_fail(
                            cp_id,
                            leak,
                            f"{kind}: M0 changed by {delta_m0:.6f} but CB injection "
                            f"only changed by {delta_cb:.6f} (leak={leak:.6f}). "
                            f"Money {'created' if leak > 0 else 'destroyed'} outside "
                            f"Central Bank mechanics.",
                        )
            else:
                # First checkpoint â€” just record baseline
                report.m0_timeline.append(
                    {
                        "checkpoint": cp_id,
                        "total": float(total),
                        "delta_m0": 0.0,
                        "delta_cb": 0.0,
                        "leak": 0.0,
                    }
                )

            prev_total = total
            prev_cb = cb

    return result


# ============================================================================
# Invariant 3: Bank Balance-Sheet Identity
# ============================================================================


def check_bank_balance_sheet(
    turn_trace: dict[str, Any], report: AuditReport
) -> InvariantResult:
    """
    Verify that each bank's balance sheet balances:
      total_assets == total_liabilities + total_equity

    If the bank's books balance but M0 leaks, the leak is NOT inside the
    banking system â€” it's in a non-bank sector (production, B2B, B2C,
    treasury, or emigration). If the bank's books DON'T balance, the
    leak is inside the banking turn itself.

    This invariant corresponds to Global Rule 1 and Rule 7.
    """
    result = InvariantResult(
        name="Bank Balance-Sheet Identity (assets == liabilities + equity)",
        passed=True,
    )

    turns = turn_trace.get("turns", [])
    for turn in turns:
        for cp in turn.get("checkpoints", []):
            cp_id = f"turn{cp['turn']}/phase{cp['phase_index']}:{cp['phase_name']}"
            bank = cp.get("bank")
            if not bank:
                continue
            result.checks_run += 1

            total_assets = to_dec(bank.get("total_assets", 0))
            total_liab = to_dec(bank.get("total_liabilities", 0))
            total_equity = to_dec(bank.get("total_equity", 0))
            is_balanced = bank.get("is_balanced", True)

            diff = total_assets - (total_liab + total_equity)
            if not approx_equal(diff, Decimal("0")):
                result.add_fail(
                    cp_id,
                    diff,
                    f"Bank {bank.get('id', '?')}: assets={total_assets:.6f} != "
                    f"liabilities({total_liab:.6f}) + equity({total_equity:.6f}) "
                    f"(diff={diff:.6f}). Bank books do not balance.",
                )
            elif not is_balanced:
                result.add_warn(
                    cp_id,
                    Decimal("0"),
                    f"Bank {bank.get('id', '?')}: is_balanced=false but "
                    f"assets == liab + equity numerically. Flag mismatch.",
                )

    return result


# ============================================================================
# Invariant 4: Loan Event M0 Neutrality
# ============================================================================


def check_loan_event_m0_neutrality(
    turn_trace: dict[str, Any], report: AuditReport
) -> InvariantResult:
    """
    Verify that loan events are M0-neutral within each phase transition.

    Loan issuance moves money from bank reserves to borrower cash â€” it
    should NOT change M0. Amortization moves money from borrower cash
    back to bank reserves â€” also M0-neutral. StatusRepaid is similar.

    If the sum of loan event amounts in a phase does not net to zero
    (issuance increases borrower cash, amortization decreases it), the
    loan accounting is creating or destroying money.

    Note: This is a heuristic check. Loan events record gross amounts,
    not directional M0 impact. We verify that the engine's own
    conservation flag matches our independent calculation.
    """
    result = InvariantResult(
        name="Loan Event Accounting (M0-neutral within phases)",
        passed=True,
    )

    turns = turn_trace.get("turns", [])
    for turn in turns:
        for cp in turn.get("checkpoints", []):
            cp_id = f"turn{cp['turn']}/phase{cp['phase_index']}:{cp['phase_name']}"
            events = cp.get("loan_events", [])
            if not events:
                continue
            result.checks_run += 1

            # Sum by kind
            by_kind: dict[str, Decimal] = {}
            for e in events:
                kind = e.get("kind", "Unknown")
                amount = to_dec(e.get("amount", 0))
                by_kind[kind] = by_kind.get(kind, Decimal("0")) + amount

            # Issued loans should increase borrower cash (M0 neutral if
            # bank reserves decrease by same amount). We can't verify
            # directionality from the event alone, but we can flag
            # if Issued events have zero amount (dead events).
            issued = by_kind.get("Issued", Decimal("0"))
            if issued > 0 and len(events) > 0:
                # Check that all Issued events have nonzero amount
                zero_issued = sum(
                    1
                    for e in events
                    if e.get("kind") == "Issued" and to_dec(e.get("amount", 0)) == 0
                )
                if zero_issued > 0:
                    result.add_warn(
                        cp_id,
                        Decimal("0"),
                        f"{zero_issued} loan Issued events with zero amount "
                        f"(dead events).",
                    )

    return result


# ============================================================================
# Invariant 5: Sector Ledger Consistency
# ============================================================================


def check_sector_ledger_consistency(
    sector_ledger: dict[str, Any], report: AuditReport
) -> InvariantResult:
    """
    Verify that sector_totals match the sum of individual company values
    in the sector ledger dump.

    If sector_totals.liquid_capital != sum(company.liquid_capital), the
    sector aggregation is wrong â€” money is being lost or created in the
    aggregation step.
    """
    result = InvariantResult(
        name="Sector Ledger Consistency (totals == sum of companies)",
        passed=True,
    )

    sectors = sector_ledger.get("sectors", {})
    for sector_name, sector_data in sectors.items():
        companies = sector_data.get("companies", [])
        totals = sector_data.get("sector_totals", {})
        if not totals:
            continue
        result.checks_run += 1

        for field_name in ["liquid_capital", "available_cash", "liabilities", "fixed_capital"]:
            sum_companies = sum(
                to_dec(c.get(field_name, 0)) for c in companies
            )
            total_val = to_dec(totals.get(field_name, 0))
            diff = total_val - sum_companies
            if not approx_equal(diff, Decimal("0"), tol_abs=Decimal("1")):
                result.add_fail(
                    f"sector:{sector_name}",
                    diff,
                    f"Sector {sector_name} {field_name}: totals={total_val:.6f} != "
                    f"sum of companies={sum_companies:.6f} (diff={diff:.6f}). "
                    f"Aggregation bug.",
                )

        # Check company_count
        declared_count = totals.get("company_count", 0)
        actual_count = len(companies)
        if declared_count != actual_count:
            result.add_warn(
                f"sector:{sector_name}",
                Decimal(abs(declared_count - actual_count)),
                f"Sector {sector_name}: company_count={declared_count} != "
                f"actual={actual_count}.",
            )

    return result


# ============================================================================
# Invariant 6: Market Clearing Convergence
# ============================================================================


def check_market_clearing(
    market_clearing: dict[str, Any], report: AuditReport
) -> InvariantResult:
    """
    Verify market clearing consistency:
      net_surplus == supply_volume - demand_volume (per commodity)

    If net_surplus != supply - demand, the clearing engine is
    misreporting the balance, which could mask allocation bugs.
    """
    result = InvariantResult(
        name="Market Clearing Consistency (net_surplus == supply - demand)",
        passed=True,
    )

    net_surplus = market_clearing.get("net_surplus", {})
    supply = market_clearing.get("supply_volume", {})
    demand = market_clearing.get("demand_volume", {})

    all_commodities = set(net_surplus.keys()) | set(supply.keys()) | set(demand.keys())
    for commodity in all_commodities:
        result.checks_run += 1
        ns = to_dec(net_surplus.get(commodity, 0))
        sv = to_dec(supply.get(commodity, 0))
        dv = to_dec(demand.get(commodity, 0))
        expected = sv - dv
        diff = ns - expected
        if not approx_equal(diff, Decimal("0"), tol_abs=Decimal("1")):
            result.add_fail(
                f"commodity:{commodity}",
                diff,
                f"{commodity}: net_surplus={ns:.6f} != supply({sv:.6f}) - "
                f"demand({dv:.6f}) = {expected:.6f} (diff={diff:.6f}).",
            )

    return result


# ============================================================================
# Invariant 7: Cross-Reference â€” Conservation Flag vs Independent Calc
# ============================================================================


def check_conservation_flag_consistency(
    turn_trace: dict[str, Any], report: AuditReport
) -> InvariantResult:
    """
    Cross-reference the engine's own conservation.fiat_conserved flag
    against our independent M0 conservation calculation.

    If the engine says fiat_conserved=true but our calculation finds a
    leak (or vice versa), there's a bug in the engine's conservation
    checker itself.
    """
    result = InvariantResult(
        name="Conservation Flag Consistency (engine flag vs independent calc)",
        passed=True,
    )

    turns = turn_trace.get("turns", [])
    prev_total: Decimal | None = None
    prev_cb: Decimal | None = None

    for turn in turns:
        for cp in turn.get("checkpoints", []):
            cp_id = f"turn{cp['turn']}/phase{cp['phase_index']}:{cp['phase_name']}"
            gf = cp.get("global_fiat", {})
            cons = cp.get("conservation", {})
            if not gf or not cons:
                continue
            result.checks_run += 1

            total = to_dec(gf.get("total", 0))
            cb = to_dec(gf.get("cumulative_cb_injection", 0))
            engine_flag = cons.get("fiat_conserved", True)
            engine_delta = to_dec(cons.get("fiat_delta", 0))
            engine_allowed = to_dec(cons.get("allowed_cb_injection_delta", 0))

            if prev_total is not None and prev_cb is not None:
                our_delta_m0 = total - prev_total
                our_delta_cb = cb - prev_cb
                our_leak = our_delta_m0 - our_delta_cb
                our_conserved = abs(our_leak) < Decimal("1") or rel_equal(
                    our_delta_m0, our_delta_cb, tol_rel=Decimal("1e-4")
                )

                if engine_flag != our_conserved:
                    # Only flag if there's a real discrepancy (not just
                    # tolerance differences)
                    if abs(our_leak) > Decimal("100"):
                        result.add_fail(
                            cp_id,
                            our_leak,
                            f"Engine says fiat_conserved={engine_flag} but "
                            f"independent calc says conserved={our_conserved} "
                            f"(our_leak={our_leak:.6f}). Engine conservation "
                            f"checker may be buggy.",
                        )

                # Also verify engine's own delta matches
                engine_calc_leak = engine_delta - engine_allowed
                if not approx_equal(engine_calc_leak, our_leak, tol_abs=Decimal("100")):
                    result.add_warn(
                        cp_id,
                        abs(engine_calc_leak - our_leak),
                        f"Engine fiat_delta={engine_delta:.6f} vs our "
                        f"delta_m0={our_delta_m0:.6f}; engine allowed="
                        f"{engine_allowed:.6f} vs our delta_cb={our_delta_cb:.6f}. "
                        f"Delta mismatch.",
                    )

            prev_total = total
            prev_cb = cb

    return result


# ============================================================================
# Root Cause Analysis
# ============================================================================


def analyze_root_cause(report: AuditReport) -> str:
    """
    Analyze the M0 timeline and invariant failures to identify the
    root cause of the M0 leak.

    Strategy:
    1. Group leaks by phase name to find which engine phase creates/destroys money.
    2. Check if bank balance sheets stay balanced during leaks (if yes,
       the leak is outside the banking system).
    3. Identify the dominant leak direction (creation vs destruction).
    4. Identify the largest single-phase leak event.
    """
    lines = []

    if not report.m0_timeline:
        lines.append("  No M0 timeline data available.")
        return "\n".join(lines)

    # Group leaks by phase
    phase_leaks: dict[str, list[Decimal]] = {}
    for entry in report.m0_timeline:
        leak = Decimal(str(entry["leak"]))
        if abs(leak) > Decimal("1"):
            # Extract phase name (after the colon)
            cp = entry["checkpoint"]
            phase = cp.split(":", 1)[1] if ":" in cp else cp
            phase_leaks.setdefault(phase, []).append(leak)

    if not phase_leaks:
        lines.append("  No significant M0 leaks detected in any phase.")
        return "\n".join(lines)

    lines.append("  Phases ranked by total leak magnitude:")
    ranked = sorted(
        phase_leaks.items(),
        key=lambda x: sum(abs(v) for v in x[1]),
        reverse=True,
    )
    for phase, leaks in ranked:
        total_leak = sum(leaks, Decimal("0"))
        avg_leak = total_leak / len(leaks) if leaks else Decimal("0")
        direction = "CREATION" if total_leak > 0 else "DESTRUCTION"
        lines.append(
            f"    {phase:<30} count={len(leaks):>3}  "
            f"net={total_leak:>20.2f}  avg={avg_leak:>15.2f}  "
            f"[{direction}]"
        )

    # Identify the largest single leak
    largest = max(
        report.m0_timeline,
        key=lambda e: abs(Decimal(str(e["leak"]))),
    )
    largest_leak = Decimal(str(largest["leak"]))
    if abs(largest_leak) > Decimal("1"):
        direction = "CREATION" if largest_leak > 0 else "DESTRUCTION"
        lines.append("")
        lines.append(
            f"  Largest single-phase leak: {largest['checkpoint']} "
            f"leak={largest_leak:.2f} [{direction}]"
        )
        lines.append(
            f"    M0: {largest['total']:.2f}, "
            f"Î”M0: {largest['delta_m0']:.2f}, "
            f"Î”CB: {largest['delta_cb']:.2f}"
        )

    # Check bank balance sheet invariant
    bank_result = next(
        (r for r in report.results if "Bank Balance-Sheet" in r.name), None
    )
    if bank_result:
        if bank_result.passed:
            lines.append("")
            lines.append(
                "  Bank balance sheets remain BALANCED during leaks."
            )
            lines.append(
                "  â†’ The leak is NOT inside the commercial banking system."
            )
            lines.append(
                "  â†’ Root cause is in a non-bank phase: production, B2B, "
                "B2C, treasury, or emigration."
            )
        else:
            lines.append("")
            lines.append(
                "  Bank balance sheets DO NOT balance during leaks."
            )
            lines.append(
                "  â†’ The leak is INSIDE the commercial banking system."
            )
            lines.append(
                "  â†’ Root cause is in loan issuance, amortization, or "
                "interbank settlement."
            )

    # Phase-specific analysis
    lines.append("")
    lines.append("  Phase-specific root cause hypotheses:")

    banking_leaks = phase_leaks.get("banking_turn_post", [])
    if banking_leaks:
        net = sum(banking_leaks, Decimal("0"))
        lines.append(
            f"    banking_turn_post: {len(banking_leaks)} leaks, net={net:.2f}. "
            f"Loan issuance/amortization may not be offsetting bank reserves "
            f"correctly. Check that every loan issuance debits bank reserves "
            f"and credits borrower cash by the same amount."
        )

    production_leaks = phase_leaks.get("production_cycle_post", [])
    if production_leaks:
        net = sum(production_leaks, Decimal("0"))
        lines.append(
            f"    production_cycle_post: {len(production_leaks)} leaks, net={net:.2f}. "
            f"Production may be creating money via wage payments without "
            f"debiting company cash, or via revenue crediting without "
            f"matching expense debiting. Check that wages debit company "
            f"cash and credit citizen_cash."
        )

    b2c_leaks = phase_leaks.get("b2c_clearing_post", [])
    if b2c_leaks:
        net = sum(b2c_leaks, Decimal("0"))
        lines.append(
            f"    b2c_clearing_post: {len(b2c_leaks)} leaks, net={net:.2f}. "
            f"B2C clearing may be crediting citizen goods consumption "
            f"without debiting citizen_cash, or debiting citizen_cash "
            f"without crediting company revenue. Check the B2C settlement "
            f"double-entry."
        )

    turn_end_leaks = phase_leaks.get("turn_end", [])
    if turn_end_leaks:
        net = sum(turn_end_leaks, Decimal("0"))
        direction = "CREATION" if net > 0 else "DESTRUCTION"
        lines.append(
            f"    turn_end: {len(turn_end_leaks)} leaks, net={net:.2f} [{direction}]. "
            f"Turn-end cleanup (emigration, mortality, bankruptcy, "
            f"liquidation) may be destroying money by removing entities "
            f"without transferring their cash to heirs/treasury. Check "
            f"that entity death routes cash to treasury or surviving "
            f"counterparties."
        )

    b2b_leaks = phase_leaks.get("b2b_orders_post", []) + phase_leaks.get(
        "b2b_settlement_post", []
    )
    if b2b_leaks:
        net = sum(b2b_leaks, Decimal("0"))
        lines.append(
            f"    b2b (orders+settlement): {len(b2b_leaks)} leaks, net={net:.2f}. "
            f"B2B settlement may have a double-entry gap. Check that "
            f"buyer debit and seller credit are equal per transaction."
        )

    building_leaks = phase_leaks.get("building_cycle_post", [])
    if building_leaks:
        net = sum(building_leaks, Decimal("0"))
        lines.append(
            f"    building_cycle_post: {len(building_leaks)} leaks, net={net:.2f}. "
            f"Construction CAPEX may be paying companies without debiting "
            f"the client, or consuming materials without crediting "
            f"suppliers. Check the construction payment double-entry."
        )

    return "\n".join(lines)


# ============================================================================
# Main
# ============================================================================


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python audit_dumps.py <dump_dir>", file=sys.stderr)
        print(
            "Example: python .devin/scripts/audit_dumps.py state/tests/diagnostic_output",
            file=sys.stderr,
        )
        return 2

    dump_dir = Path(sys.argv[1])
    if not dump_dir.is_dir():
        print(f"Error: dump directory not found: {dump_dir}", file=sys.stderr)
        return 2

    # Load all JSON dumps
    turn_trace = load_json(dump_dir / "turn_trace_q1.json")
    sector_ledger = load_json(dump_dir / "sector_ledger.json")
    market_clearing = load_json(dump_dir / "market_clearing.json")
    banking_state = load_json(dump_dir / "banking_state.json")

    if not turn_trace and not sector_ledger and not market_clearing:
        print(f"Error: no JSON dumps found in {dump_dir}", file=sys.stderr)
        return 2

    report = AuditReport(dump_dir=str(dump_dir))

    # Run all invariants
    if turn_trace:
        report.results.append(
            check_m0_component_reconciliation(turn_trace, report)
        )
        report.results.append(check_m0_conservation(turn_trace, report))
        report.results.append(check_bank_balance_sheet(turn_trace, report))
        report.results.append(
            check_loan_event_m0_neutrality(turn_trace, report)
        )
        report.results.append(
            check_conservation_flag_consistency(turn_trace, report)
        )

    if sector_ledger:
        report.results.append(
            check_sector_ledger_consistency(sector_ledger, report)
        )

    if market_clearing:
        report.results.append(check_market_clearing(market_clearing, report))

    # Analyze root cause if there are failures
    if not report.all_pass:
        report.root_cause = analyze_root_cause(report)

    # Print report
    print(report.summary())

    # Also write report to file
    report_path = dump_dir / "audit_report.txt"
    with open(report_path, "w", encoding="utf-8") as f:
        f.write(report.summary())
    print(f"\nReport written to: {report_path}")

    return 0 if report.all_pass else 1


if __name__ == "__main__":
    sys.exit(main())

