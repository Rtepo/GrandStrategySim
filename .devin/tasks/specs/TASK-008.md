# TASK-008 — Diplomacy missions & distinct international resolutions

## Defect
Diplomats/ambassadors/spies exist in the backend but the Diplomacy view shows
no per-agent mission list; every international organisation renders the same
cloned resolutions (a 1900-era game showing an "Emission Standards Accord"),
and resolutions are not expandable to their actual rules/effects/funding.

## Required behaviour
1. **Diplomacy Missions table**: each active Diplomat, Ambassador and Spy
   with assigned target country, current mission type (Improve Relations,
   Negotiate Trade Pact, Steal Industrial Tech, Sabotage, …) and mission
   progress.
2. **Distinct era-appropriate resolutions**: each organisation (World Forum,
   Slavic Compact, …) gets its own resolution set appropriate to 1900 — no
   cross-org cloning, no anachronisms.
3. **Expandable resolutions**: every resolution expands to a full
   description — statutory rules, economic effects, funding sources.

## Acceptance
- `verify_task.sh TASK-008` green.
- `world_forum_resolutions != regional_bloc_resolutions`.

## Files
- `state/src/international/organizations.rs` (resolution catalogues),
  `state/src/ui/snapshot.rs` (DiplomatRow mission join, DirectiveRow
  expansion fields)
- `src/pages/DiplomacyPage.tsx` (international organisations + their
  Assembly resolutions render inside Diplomacy's IA panel —
  `OrganizationDetailPage` is for domestic guilds/unions/movements)
