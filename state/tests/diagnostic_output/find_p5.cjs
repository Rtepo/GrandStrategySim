const fs = require('fs');
const lines = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/src/engine/turn.rs', 'utf8').split('\n');
const p3 = 1290; // b2b_settlement_post
const p5 = 2017; // production_cycle_post
for (let i = p3 - 1; i < p5 - 1; i++) {
    const l = lines[i];
    if (!l) continue;
    if (l.match(/liquid_reserves\s*\+=/) || l.match(/budget\.liquid_reserves/) || l.match(/savings\s*\+=/)) {
        if (!l.match(/^\s*\/\//)) {
            console.log(`${i + 1}: ${l.trim().substring(0, 150)}`);
        }
    }
}
