const fs = require('fs');
const content = fs.readFileSync('C:/Users/netse/Downloads/SillyElaborateState/test_run11.txt', 'utf8');
const lines = content.split(/\r?\n/);
for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes('TURN_END_IMBALANCE') && lines[i].includes('turn=0') && lines[i].includes('BANK-ELD-002')) {
        // Print this line and next 3 lines (in case it's wrapped)
        let combined = lines[i];
        for (let j = 1; j <= 3; j++) {
            if (i + j < lines.length && !lines[i+j].includes('TURN_END_IMBALANCE') && !lines[i+j].includes('test_')) {
                combined += lines[i+j];
            }
        }
        console.log(combined);
    }
}
