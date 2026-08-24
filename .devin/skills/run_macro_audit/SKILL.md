---
name: run_macro_audit
description: Audit a Technical Execution Plan against the 21 Global Directives before presenting it to the user
triggers:
  - model
allowed-tools:
  - read
  - grep
  - glob
---

# Macro-Architectural Audit

You must perform a rigorous self-audit of the current Technical Execution Plan against the 21 Global Directives. This audit runs IMMEDIATELY AFTER you generate a new Technical Execution Plan, BEFORE presenting it to the user.

## Audit Checklist

Read the plan file and verify each of the following directives. For each, state PASS or FAIL with a specific explanation.

### 1. Mass Conservation (Rules 1, 14, 15)
- Does every physical transformation have input mass = output mass + waste + emissions?
- Are there any points where physical material disappears or appears from nothing?
- Are all rates scaled by physical dimensions (capacity, workers, floor area, throughput) rather than flat constants?

### 2. Double-Entry Bookkeeping (Rule 1, 7)
- For every new cost, fee, or price: who is the exact debit counterparty?
- For every new revenue, income, or payment: who is the exact credit counterparty?
- Is there any cash flow that goes to/from "the void" without a counterparty?
- Are individual entity ledgers maintained (no averaging/communization)?

### 3. No Teleportation (Logistics)
- Are freight costs included for all physical movements between locations?
- Do commodities that physically move consume FreightCapacity or equivalent logistics?
- Is there any implicit assumption that goods appear at their destination without transport?

### 4. Clamping (Hard Bounds)
- Are there hard caps (0% and 100%, or 0.0 and max_capacity) for all new buffers, inventories, and utilization rates?
- Can any new field go negative when it should be clamped to zero?
- Can any new field exceed its physical maximum?

### 5. No Magic Numbers (Rule 2)
- Are all thresholds, costs, and fees derived from dynamic macroeconomic variables (average_wage, market prices, capital intensity)?
- Are there any hardcoded nominal floats (e.g., 10000.0) used as costs or thresholds?

### 6. Technological Matrices (Rule 13)
- Does every new building/plant type have distinct Production, Automation, and Organization method slots?
- Are fundamentally different operational structures under separate registry keys?

### 7. Architectural Parsimony (Rule 14)
- Does the plan extend existing engine systems rather than creating parallel ones?
- Are there redundant or overlapping systems that should be consolidated?

### 8. Temporal Causality (Rule 16)
- Does the plan specify which turn-loop phase each new computation belongs to?
- Are there any temporal paradoxes (applying a buff to a phase that already executed)?

### 9. Asymmetric Information (Rule 11)
- Are snapshot DTOs role-gated? Does the backend physically strip hidden data?
- Is there any hidden data sent to the frontend that is merely concealed by UI?

### 10. Full-Stack Accountability (Rule 17)
- Are frontend components and snapshot DTOs planned for every new feature?
- Is there any backend feature that has no UI visibility plan?

### 11. Complete Entity Lifecycle (Rule 4)
- Does every new entity/structure have defined Birth, Life, and Death?
- Are there immortal structures or dead code loops?

### 12. Market Forces (Rule 5)
- Are resource distributions using competitive mechanics (market clearing or pro-rata)?
- Are there any hardcoded percentage splits (e.g., 50/50)?

### 13. Rational Actors (Rule 8)
- Do all agents act to maximize utility/profit?
- Is there any charity or debt forgiveness without explicit state welfare mechanics?

## Output Format

Append the following section to the END of the plan markdown file:

```
## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS/FAIL | ... |
| Double-Entry Bookkeeping | PASS/FAIL | ... |
| No Teleportation | PASS/FAIL | ... |
| Clamping | PASS/FAIL | ... |
| No Magic Numbers | PASS/FAIL | ... |
| Technological Matrices | PASS/FAIL | ... |
| Architectural Parsimony | PASS/FAIL | ... |
| Temporal Causality | PASS/FAIL | ... |
| Asymmetric Information | PASS/FAIL | ... |
| Full-Stack Accountability | PASS/FAIL | ... |
| Complete Entity Lifecycle | PASS/FAIL | ... |
| Market Forces | PASS/FAIL | ... |
| Rational Actors | PASS/FAIL | ... |

### Summary
- Total PASS: N/13
- Total FAIL: N/13
- Critical Issues: [list any FAIL items that must be fixed before implementation]
```

If ANY directive fails, you must fix the plan before presenting it to the user. Do not present a plan with known FAIL items.
