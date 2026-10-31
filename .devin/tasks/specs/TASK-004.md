# TASK-004 — Statutes prose, Intelligence sub-tab, expandable treaties, deeper press

## Defects
1. Active statutes render raw serializations containing `_`, `: true`,
   `: false` (backend cleanup landed but UI/strings regressed or weren't
   wired — verify actual render path).
2. WS7 backend (`CovertOperationRow`, intelligence agency DTO, operations)
   is on `main` but `DiplomacyPage.tsx` has no Intelligence & Espionage
   sub-tab — `covert` appears only in `api.ts`.
3. Treaties render flat; negotiation logs exist backend-side but are not
   surfaced.
4. Newspaper articles are thin single-paragraph blurbs.

## Required behaviour
1. Statutes (ParliamentPage): render human prose — title-cased names,
   boolean provisions as "Enabled/Active/…" phrasing; zero `_` / `: true` /
   `: false` in rendered strings.
2. Diplomacy → Intelligence & Espionage sub-tab: per-target operations list
   (mission type, ring size, progress, status, outcome), expandable.
3. Treaties expandable → negotiation log timeline + signatories + terms.
4. Press articles: multi-paragraph bodies, expandable full text.

## Acceptance
- `verify_task.sh TASK-004` green.
- Manual sanity: statute strings contain no `_` outside known identifiers.
