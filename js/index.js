const { existsSync } = require('fs');
const { join } = require('path');

// Load the native addon that napi-rs builds
const dir = __dirname;
const ext = process.platform === 'win32' ? '.node' :
            process.platform === 'darwin' ? '.dylib' : '.so';

// napi-rs outputs platform-specific .node files: tson.win32-x64-gnu.node etc.
const candidates = [
    join(dir, `tson.${process.platform}-${process.arch}-gnu${ext}`),
    join(dir, `tson.${process.platform}-${process.arch}${ext}`),
    join(dir, `index${ext}`),
];

let native;
for (const p of candidates) {
    if (existsSync(p)) {
        native = require(p);
        break;
    }
}

if (!native) {
    throw new Error(`TSON native addon not built. Seached: ${candidates.join(', ')}\n` +
        'Run: npx napi build --platform --release --output-dir js');
}

module.exports = native;
