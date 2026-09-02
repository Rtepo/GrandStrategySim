const fs = require('fs');
const lines = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/state/src/engine/turn.rs', 'utf8').split('\n');
// Find DIAGNOSTIC CHECKPOINT lines
const checkpoints = [];
for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes('DIAGNOSTIC CHECKPOINT')) {
        checkpoints.push({ line: i + 1, text: lines[i].trim() });
    }
}
console.log('Checkpoints:');
checkpoints.forEach(c => console.log(`  ${c.line}: ${c.text}`));

// Find P5 and P6
const p5cp = checkpoints.find(c => c.text.includes('production_cycle_post'));
const p6cp = checkpoints.find(c => c.text.includes('b2c_clearing_post'));
if (!p5cp || !p6cp) { console.log('Could not find checkpoints'); process.exit(1); }
console.log(`\nSearching between P5 (line ${p5cp.line}) and P6 (line ${p6cp.line})`);
for (let i = p5cp.line - 1; i < p6cp.line - 1; i++) {
    if (lines[i] && lines[i].match(/savings/) && lines[i].match(/\+=|=/)) {
        console.log(`${i + 1}: ${lines[i].trim()}`);
    }
}
