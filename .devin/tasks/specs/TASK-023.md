# TASK-023 — getBoundingClientRect crash, tab dedupe, VIP ministers, cultural coherence (v4.30.0)

- Null-guard every DOM ref access in DynastyTree wheel/mouse handlers (`ref.current?.getBoundingClientRect()` + early return) — crash "Cannot read properties of null" eliminated; portal tooltip stays.
- Remove duplicate Royal Dynasty sub-tab from Parliament (keep in Government only); remove Finance from left nav, embed as Government sub-tab (Cabinet & Overview | Ministries | Public Finance | Royal Dynasty).
- Every minister backed by a real VIP (clickable link + tooltip). Bind each of the 16 countries to ONE consistent cultural name-bank for VIPs, parties, companies, banks, regiments, cities; fix demonyms (Anglians not Anglia).
