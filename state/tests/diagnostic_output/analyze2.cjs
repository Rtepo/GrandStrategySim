const fs = require('fs');
const d = JSON.parse(fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/tests/diagnostic_output/turn_trace_q1.json', 'utf8'));

// Find turn 0 checkpoints and show M0 components
for (const t of d.turns || []) {
    if (t.turn !== 0) continue;
    for (const cp of t.checkpoints || []) {
        const f = cp.global_fiat || {};
        console.log(`T${cp.turn} P${cp.phase_index} (${cp.phase_name}): total=${(f.total||0).toFixed(2)} treasury=${(f.treasury_cash||0).toFixed(2)} citizen=${(f.citizen_cash||0).toFixed(2)} bank_reserves=${(f.bank_reserves||0).toFixed(2)} offshore=${(f.offshore_capital||0).toFixed(2)} see=${(f.see_charity_pool||0).toFixed(2)} cb_inj=${(f.cumulative_cb_injection||0).toFixed(2)}`);
    }
}
