# TASK-019 — Dynasty Tree Zoom Crash & Tooltip Clipping

1. **Zoom/pan crash**: audit pan/zoom handlers — clamp scale to
   [0.35, 3.0], guard NaN/Infinity/null coordinates and invalid hook
   calls, prevent wheel-event propagation issues.
2. **Bottom-row tooltip clipping**: tooltips via fixed/portal layer or
   dynamic upward flip (bottom-full) with overflow: visible — lowest
   generation never cut by the card frame.
