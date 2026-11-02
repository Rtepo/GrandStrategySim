# TASK-015 — CB Rate Smoothing, Game-World FX Currencies & Expandable MPC Votes

## Context
Policy rate jumps ±2000bps in one meeting; every country clones Dual Mandate /
80% independence; FX reserves list EUR/USD/GBP instead of simulated-nation
currencies; gold tonnage absurd (208,367t); MPC rows not expandable.

## Requirements
1. **Rate smoothing**: r_t = ρ·r_{t-1} + (1−ρ)·r*_t with per-meeting step cap
   ±25–100bps.
2. **Differentiated mandates/independence**: mandate (Gold Standard / Price
   Stability / Dual Mandate / FX Peg / Treasury Financing) and independence
   15–95% vary by regime + economic law.
3. **Real FX currencies**: reserve table holds actual simulated-nation
   currencies (Lechian Marka, Eldorian Crown, …) with exchange rates — no
   EUR/USD/GBP. Gold tonnage 50–2,500t.
4. **Expandable MPC rows**: click to expand → economic briefing (CPI, output
   gap, credit growth) + per-member vote/trait/dissent.

## Rust assertions
- max single-turn rate change ≤ 150bps.
- No "EUR"/"USD"/"GBP" in FX reserve rows; currencies match generated nations.
- Gold reserves within 50–2,500t.
- Mandates/independence vary across countries (≥3 distinct mandates, range ≥40pp).

## UI checks
- FX table + expandable MPC rows in BankingPage.
