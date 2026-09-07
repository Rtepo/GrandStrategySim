const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const syncFile = path.join(process.cwd(), 'agents_sync.json');
const greenCommit = execSync('git rev-parse HEAD', { encoding: 'utf8' }).trim();

function readJson(f) {
    let raw = fs.readFileSync(f, 'utf8');
    if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
    return JSON.parse(raw);
}

let sync = readJson(syncFile);
sync.last_green_commit = greenCommit;

const tmp = syncFile + '.tmp';
fs.writeFileSync(tmp, JSON.stringify(sync, null, 2));
fs.renameSync(tmp, syncFile);

console.log('Updated last_green_commit to: ' + greenCommit);
