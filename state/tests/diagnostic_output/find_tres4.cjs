const fs = require('fs');
const lines = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/src/engine/turn.rs', 'utf8').split('\n');
const p6 = 3808;
const p4 = 6423;
for (let i = p6 - 1; i < p4 - 1; i++) {
    const l = lines[i];
    if (!l) continue;
    if (l.match(/route_tax_collection|settle_transfer_to_treasury|credit.*treasury|treasury.*credit/i) && !l.match(/^\s*\/\//)) {
        console.log(`${i + 1}: ${l.trim().substring(0, 150)}`);
    }
}
