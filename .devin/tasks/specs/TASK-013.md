# TASK-013 — Two-Column Battle Line UI, Aggressor/Defender & No Naval Units on Land

## Context
War rows don't badge initiator vs defender; expanded battles show a flat
comma-separated attacker/defender line; UNIT-* fallback IDs leak; naval
battleships appear in inland land battles.

## Requirements
1. **Aggressor/Defender badges**: Active Wars & Historical Wars explicitly
   badge Initiator/Aggressor vs Defender + concrete war goal.
2. **Two-column battle line table**: below the casualty bar in each expanded
   battle — left column attacker line vs right column defender line:
   - Header per side: Commanding General (VIP link), Tactical Doctrine, Total
     Attack Power, Total Defense Power, Avg Morale, Combined Arms Bonus.
   - Unit rows with type badges ([INF]/[ART]/[CAV]/[ENG]/[NAV] coastal only)
     and per-unit strength/losses.
3. **Purge UNIT-* fallbacks**: unresolved unit ids render culturally-named
   fallbacks, never raw "UNIT-…" strings.
4. **No naval units on land**: battle unit selection excludes Naval units in
   non-coastal/non-sea engagements.

## Rust assertions
- WarBattleRow exposes structured attacker/defender lineup rows (name, type
  badge, power, morale, losses).
- No "UNIT-" prefixed names in snapshot battle rows.
- Naval units never appear in land-locked battle lineups.

## UI checks
- Two-column grid table in expanded battle.
- Aggressor/Defender chips on war rows.
