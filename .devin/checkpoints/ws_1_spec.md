# WS1 Spec — Calendar Math Fix & 1900 Tech-Era Commodity Gating

## WS1.1 Calendar (January rollover, Aug-1900 start = offset 14)

**Model:** `global_turn` = half-months COMPLETED (0 at genesis). Calendar fields describe
the CURRENT (in-progress) half-month: `h = global_turn + S` where `S = (start_month-1)*2`.
With `start_month=8` (August): h=14 → Aug-early-1900 at genesis; h=24 → Jan-early-1901 at
turn 10 (matches release assertion `turn_10 → Jan 1901`, crucible samples `years[turn_num]`
BEFORE each `run_turn_inner`, i.e. at global_turn=10).

Formula: `year = start_year + h/24`, `month = (h/2)%12 + 1`, `half_month = h%2 == 1`.

### Edits
1. `state/src/state/mod.rs` (~128-159): `advance()` → h-based formula;
   `is_year_end()` → `h%24==23` (Dec-late); `is_year_start()` → `h%24==0` (Jan-early);
   add `fn absolute_half_month(&self) -> u32` helper.
2. `state/src/engine/turn.rs:14003-14019`: post-turn sync → same h-based formula
   (handles global_turn==0 via same math, no special case needed since h=14>0).
3. `state/src/engine/turn.rs:13526-13531`: year-end trigger `turn%24==0` →
   `(turn + S)%24==0` (fires at turn=10,34,… = entering January) → `year+=1` + fiscal
   settlement aligned to January.
4. `state/src/engine/turn.rs:10496`: `is_year_boundary` = `(turn+1+S)%24==0` (fires on
   the Dec-late turn whose processing closes the year — process_political_year stays
   once/year).
5. `state/src/engine/generator/mod.rs:199-201`: `start_month`/`current_month` =
   8 for Y1900 scenario (keep 9 for other StartYears — minimal blast radius).
6. `state/src/io/save_manager.rs:304-311`: on-load month/half_month → h-based formula.
7. `state/src/agriculture.rs:61`: `global_turn % 24` → `(global_turn + S) % 24`
   (position-in-year for sowing windows).
8. `1925 + turn/24` hardcoded year bugs → real calendar year:
   - `retail.rs:767` (build_consumer_demand) — pass `current_year` param (callers
     retail.rs:3020,3076 + turn.rs:8248).
   - `corporate/manager.rs:615,658`, `politics/anti_corruption.rs:332` — same pattern.
   - `politics/bill_lifecycle.rs:1771-1773` `format!("Budget Bill FY{}", 1900+year)` →
     calendar year.
9. DEFER to WS2: `LegislativeSession::new(turn/24)` session indexing (bill_lifecycle.rs:1591,
   politics/turn.rs:1076) — sim-year ordinal; evaluate during parliament work.
10. `macro_data.rs:678` `year: turn/24` telemetry label — check consumer; if it's a
    calendar label fix to current_year, else leave as sim-year index.

## WS1.2 Tech-Era Commodity Gating + Fertilizer Supply

**New single source of truth:** `Commodity::introduction_year() -> u32` in
`state/src/registries/enums.rs`. Table (historical, all others → 0):
- Fertilizers 1880 (superphosphate era; unlocks the existing 1880 "Fertilizer
  Production" pm → seeds chemical plants at 1900 per mandate)
- Cars 1910, OfficeMachinery 1905 (typewriter/mech_008 era), Radio 1920,
  Televisions 1936, Agd 1930, Plastics 1950, ElectronicComponents 1950,
  Semiconductors 1975, Software 1975, Batteries 1950, PhotovoltaicPanels 1975.

### Edits
1. `enums.rs`: add `introduction_year()`.
2. `corporate.rs:3678-3692` `is_era_appropriate_commodity` → delegate to
   `c.introduction_year() <= start_year` (removes the Fertilizers≥1925 block).
3. `retail.rs` `era_consumption_multiplier`: first line
   `if year < commodity.introduction_year() { return 0.0 }` — covers
   Software/Plastics/OfficeMachinery/Batteries which currently fall to `_=>1.0`.
4. `retail.rs` `build_consumer_demand` + both call sites + turn.rs:8248 — pass
   `current_year` (from `state.calendar.current_year`) instead of `1925+turn/24`.
5. `corporate.rs:7040-7176` `state_building_recipe`: era-aware inputs —
   police_station/service_headquarters/intelligence_hq drop `Cars` (<1910) and
   `ElectronicComponents` (<1950); substitute Paper/MechanicalComponents/Food so
   B2B demand at 1900 has no anachronisms.
6. `snapshot.rs:3549` CommodityRow loop: `filter` out
   `c.introduction_year() > current_year` (hide uninvented goods from Market UI —
   need `state.calendar.current_year` in scope).
7. Audit: methods ≤1900 whose INPUTS include gated commodities — the two
   `!inputs.iter().any(...)` filters at corporate.rs:3280-3290/3375-3385 already
   exclude ElectronicComponents/Semiconductors/Software inputs in that picker;
   extend to the full intro-year table.

## Release-gate test
New `state/tests/epics/v4220_release_gate_test.rs` (+ `[[test]]` block in
state/Cargo.toml with `required-features=["epic-tests"]`): Y1900 world,
120 turns, asserts calendar/demand/morale/fog/society/press gates.
(Crucible stays Y1925 — mandate assertions are 1900-specific.)

## Risk notes
- Fiscal settlement moving Aug→Jan shifts timing of CapitalGains year-end by ~5 months;
  acceptable per mandate.
- `tech_ok`/`pm.year` filters elsewhere already gate method choice correctly.
- Save compat: calendar fields are serde defaults; old saves get recomputed
  month/year on next turn sync — acceptable drift of ≤5 months once.
