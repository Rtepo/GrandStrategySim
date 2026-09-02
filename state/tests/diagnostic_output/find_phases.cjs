const fs = require('fs');
const lines = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/src/engine/turn.rs', 'utf8').split('\n');
// Show all section headers between P5 (line 2017) and P6 (line 3699)
for (let i = 2016; i < 3698; i++) {
    const l = lines[i];
    if (l && (l.includes('════') || l.includes('Phase ') || l.includes('PHASE ')) && !l.includes('// ──')) {
        console.log(`${i + 1}: ${l.trim().substring(0, 100)}`);
    }
}
