// Turns the ld.lld command clang prints (-###) into one that reads the mingw
// runtime only from <mindir>, and runs it with <lld> (no clang, no VS):
// node lld-only.js <clang-hash.txt> <mindir> <lld> <out.exe>
const fs = require('fs'), path = require('path'), { spawnSync } = require('child_process');
const [hashFile, minDir, lld, out] = process.argv.slice(2);
const line = fs.readFileSync(hashFile, 'utf8').split('\n').find(l => l.includes('ld.lld'));
const args = [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map(m => m[1].replace(/\\\\/g, '\\'));
args.shift();
const mingw = /llvm-mingw/i;
const kept = [];
for (let i = 0; i < args.length; i++) {
  let a = args[i];
  if (a === '-o') { kept.push('-o', out); i++; continue; }
  if (a.startsWith('-L') && mingw.test(a)) continue;
  if (mingw.test(a)) a = path.join(minDir, path.basename(a));
  kept.push(a);
}
kept.push('-L' + minDir);
const rsp = out + '.rsp';
fs.writeFileSync(rsp, kept.map(a => `"${a.replace(/\\/g, '/')}"`).join('\n'));
const r = spawnSync(lld, ['-flavor', 'gnu', '@' + rsp], { encoding: 'utf8', maxBuffer: 1 << 28 });
console.log(r.stdout, r.stderr, r.error || '');
process.exit(r.status ?? 1);
