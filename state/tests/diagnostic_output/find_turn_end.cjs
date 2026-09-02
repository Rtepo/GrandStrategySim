const fs = require('fs');
const lines = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/src/engine/turn.rs', 'utf8').split('\n');
const checkpoints = [];
for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes('DIAGNOSTIC CHECKPOINT')) {
        checkpoints.push({ line: i + 1, text: lines[i].trim() });
    }
}
const p6cp = checkpoints.find(c => c.text.includes('b2c_clearing_post'));
const p4cp = checkpoints.find(c => c.text.includes('turn_end'));
console.log(`Searching between P6 (line ${p6cp.line}) and P4 (line ${p4cp.line})`);
// Show all Phase headers and treasury/reserve modifications
for (let i = p6cp.line - 1; i < p4cp.line - 1; i++) {
    const l = lines[i];
    if (!l) continue;
    if (l.match(/liquid_reserves\s*\+=|liquid_reserves\s*-=|liquid_reserves\s*=/) ||
        l.match(/reserves_at_central_bank\s*\+=|reserves_at_central_bank\s*-=/) ||
        l.match(/treasury.*\+=|treasury.*-=/) ||
        (l.includes('Phase ') && l.match(/^\s+\/\//))) {
        console.log(`${i + 1}: ${l.trim().substring(0, 120)}`);
    }
}
