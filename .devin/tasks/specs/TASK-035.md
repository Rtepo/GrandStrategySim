# TASK-035 — Full-bleed map + sea zones + frontlines + cession minimap (v4.32.0)

- Tooltip shows live owner_country_id; map fills viewport edge-to-edge (no oval ring); country labels scale to territory bbox; capital names hidden below 1.8x zoom.
- Inland seas connect to ocean via strait/estuary, never overlap lakes, never slice land; clickable named sea zones/basins; tinted territorial-waters/EEZ coastal bands; islands never overlap or touch mainland, owned by containing sea-zone country.
- Styled clickable frontline arcs → Frontline Inspector Card (armies, commanders, troops, supply both sides).
- Historical Wars before/after minimap: transferred regions colored defender→victor, neighbors muted grey.
