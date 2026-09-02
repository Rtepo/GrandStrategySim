const fs = require('fs');
const d = JSON.parse(fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/tests/diagnostic_output/turn_trace_q1.json', 'utf8'));

// Show M0 component deltas for turn 0
let prev = null;
for (const t of d.turns || []) {
    if (t.turn !== 0) continue;
    for (const cp of t.checkpoints || []) {
        const f = cp.global_fiat || {};
        if (prev) {
            const dt = (f.treasury_cash||0) - prev.treasury;
            const dc = (f.citizen_cash||0) - prev.citizen;
            const db = (f.bank_reserves||0) - prev.bankRes;
            const doff = (f.offshore_capital||0) - prev.offshore;
            const dsee = (f.see_charity_pool||0) - prev.see;
            const dtotal = (f.total||0) - prev.total;
            const dcb = (f.cumulative_cb_injection||0) - prev.cbInj;
            console.log(`T${cp.turn} P${cp.phase_index} (${cp.phase_name}): dTotal=${dtotal.toFixed(2)} dTreas=${dt.toFixed(2)} dCitiz=${dc.toFixed(2)} dBankRes=${db.toFixed(2)} dOffsh=${doff.toFixed(2)} dSee=${dsee.toFixed(2)} dCBInj=${dcb.toFixed(2)}`);
        } else {
            console.log(`T${cp.turn} P${cp.phase_index} (${cp.phase_name}): total=${(f.total||0).toFixed(2)} treasury=${(f.treasury_cash||0).toFixed(2)} citizen=${(f.citizen_cash||0).toFixed(2)} bank_reserves=${(f.bank_reserves||0).toFixed(2)} cb_inj=${(f.cumulative_cb_injection||0).toFixed(2)}`);
        }
        prev = {
            total: f.total||0,
            treasury: f.treasury_cash||0,
            citizen: f.citizen_cash||0,
            bankRes: f.bank_reserves||0,
            offshore: f.offshore_capital||0,
            see: f.see_charity_pool||0,
            cbInj: f.cumulative_cb_injection||0,
        };
    }
}
