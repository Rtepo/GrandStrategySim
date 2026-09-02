const fs = require('fs');
const d = JSON.parse(fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/tests/diagnostic_output/turn_trace_q1.json', 'utf8'));
const out = [];
for (const t of d.turns || []) {
    for (const cp of t.checkpoints || []) {
        for (const v of (cp.conservation || {}).violations || []) {
            if ((v.magnitude || 0) > 1.0) {
                out.push(`T${cp.turn} P${cp.phase_index} (${cp.phase_name}): ${v.kind} mag=${v.magnitude.toFixed(2)} -- ${v.explanation.slice(0, 250)}`);
            }
        }
    }
}
console.log(out.slice(0, 30).join('\n'));
console.log(`\nTotal large violations: ${out.length}`);
