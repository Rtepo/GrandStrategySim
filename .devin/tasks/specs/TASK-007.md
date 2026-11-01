# TASK-007 — Deep military overhaul: regimental names, war outcomes, navy, POWs

## Defect
Units render as `UNIT-Anatolia-004-001-001-001` and `Anglia-Region3` raw ids;
battles show `atk 0 / def 0` with no commanders, doctrine or phase detail;
the same 2-turn war (Anatolia vs Dacia) repeats 5×; wars end with no
territorial change; Readiness shows 5000% (100× scaling bug), Training 0%;
coastal countries have Navy 0; POW camps are static shells.

## Required behaviour
1. **Cultural regimental names** (`1st Anatolian Line Infantry`,
   `2nd Royal Lancers`) + real region names — no `UNIT-*` strings reach UI.
   Expanded battles show Commanding Generals (VIP links) on both sides,
   Tactical Doctrine (Artillery Barrage, Flanking Envelopment, Trench
   Defense), non-zero unit counts per phase, per-phase casualty breakdowns.
2. **Historical Wars → unified coalition war table**: group allied
   participants into ONE row per war — Initiator (Aggressor) vs Defender,
   concrete War Goals, total military+civilian casualties, material
   destruction ($), peace treaty terms, and actual Regional Ownership
   Transfers (annexation/cession) visible on the World Map after decisive
   conclusion.
3. **OOB fix + branches**: Readiness clamped 0–100% (fix 100× scale bug),
   real Training % and Experience %; coastal countries get Naval Fleets
   (Battleships, Cruisers, Destroyers, Transports) in 1900; Land Forces
   broken into Infantry, Cavalry/Hussars, Artillery, Engineers/Support.
4. **Dynamic POW camps**: prisoners captured from battles, per-turn Camp
   Maintenance Cost ($), Forced Labor output (authoritarian/wartime),
   escape attempts, health/mortality dynamics.

## Acceptance
- `verify_task.sh TASK-007` green.
- `max_army_readiness_pct <= 100.0`; `coastal_countries_with_navy > 0`;
  `!battle_units_contain("UNIT-")`.

## Files
- `state/src/military/oob.rs`, `state/src/military/pows.rs`,
  `state/src/military/combat.rs` (or `units.rs` for naming),
  `state/src/ui/snapshot.rs` (military DTO section)
- `src/pages/MilitaryPage.tsx`
