const fs = require('fs');
const lines = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/src/engine/turn.rs', 'utf8').split('\n');
const p6 = 3808;
const p4 = 6423;
for (let i = p6 - 1; i < p4 - 1; i++) {
    const l = lines[i];
    if (!l) continue;
    // Look for function calls that take country or budget
    if (l.match(/process_|allocate_|route_|settle_|credit_/) && l.match(/\(|;/) && !l.match(/^\s*\/\//)) {
        console.log(`${i + 1}: ${l.trim().substring(0, 150)}`);
    }
}
