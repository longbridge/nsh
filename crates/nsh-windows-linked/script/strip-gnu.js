// Copies the link inputs of a gnullvm link command into <dir> without
// rustc metadata (lib.rmeta), and writes a link command that uses them:
// node strip-gnu.js <linkcmd.txt> <dir> <out-linkcmd.txt>
const fs = require('fs'), path = require('path'), { spawnSync } = require('child_process');
const [cmdFile, dir, outCmd] = process.argv.slice(2);
fs.mkdirSync(dir, { recursive: true });
let line = fs.readFileSync(cmdFile, 'utf8');
let size = 0;
for (const m of [...line.matchAll(/"([^"]*\.(?:rlib|a))"/g)]) {
  const src = m[1].replace(/\\\\/g, '\\');
  if (!fs.existsSync(src)) continue;
  const out = path.join(dir, path.win32.basename(src).replace(/\.rlib$/, '.a'));
  fs.copyFileSync(src, out);
  if (src.endsWith('.rlib')) {
    const r = spawnSync('llvm-ar', ['d', out, 'lib.rmeta'], { encoding: 'utf8' });
    if (r.status) { console.log(src, r.stderr); process.exit(1); }
  }
  size += fs.statSync(out).size;
  line = line.split(m[0]).join(`"${out.replace(/\\/g, '/')}"`);
}
fs.writeFileSync(outCmd, line);
console.log((size / 1048576).toFixed(1), 'MB in', dir);
