const fs = require('fs');
const lines = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/src/engine/turn.rs', 'utf8').split('\n');
const p6 = 3808; // b2c_clearing_post
const p4 = 6423; // turn_end
for (let i = p6 - 1; i < p4 - 1; i++) {
    const l = lines[i];
    if (!l) continue;
    if (l.match(/liquid_reserves\s*\+=/) || l.match(/liquid_reserves\s*=/)) {
        console.log(`${i + 1}: ${l.trim().substring(0, 150)}`);
    }
}
