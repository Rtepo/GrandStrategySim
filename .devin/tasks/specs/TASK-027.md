# TASK-027 — World map completeness, lake carving, ocean canvas, contiguous annexations (v4.31.0)

- 100% of simulation regions have valid non-degenerate polygons rendered; inspector binds live owner_country_id + shows regional capital city name.
- Lakes carved into terrain before city/capital placement; roads/rails never inside water; no capital submerged.
- Full-canvas ocean background replaces oval ribbon; no offshore islands for landlocked countries.
- Label font clamped by polygon area; region/capital labels hidden at low zoom; larger canvas.
- Peace-treaty cessions only transfer regions adjacent (land/strait) to victor territory — no exclaves.
Gate: rendered_map_regions_count == total_simulation_regions_count && capitals_covered_by_lakes_count == 0.
