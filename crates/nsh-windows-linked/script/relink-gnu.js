// Relinks the gnullvm runner with only the given parts:
// node relink-gnu.js <linkcmd.txt> <out.exe> [editor] [plot]
const fs = require('fs'), { spawnSync } = require('child_process');
const [cmdFile, out, ...parts] = process.argv.slice(2);
const line = fs.readFileSync(cmdFile, 'utf8').trim();
const args = [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map(m => m[1].replace(/\\\\/g, '\\').replace(/\\"/g, '"'));
const exe = args.shift();
const all = ['editor', 'plot'];
const kept = [];
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === '-o') { i++; continue; }
  const m = /libnsh_windows_([a-z]+)(-[0-9a-f]+)?\.(rlib|a)$/.exec(a);
  if (m && all.includes(m[1]) && !parts.includes(m[1])) continue;
  kept.push(a);
}
for (const p of all) if (!parts.includes(p)) kept.push(`-Wl,-Xlink=-alternatename:nsh_part_${p}=nsh_part_none`);
kept.push('-o', out);
if (process.env.EXTRA) kept.push(...process.env.EXTRA.split(' '));
const rsp = out + '.rsp';
fs.writeFileSync(rsp, kept.map(a => `"${a.replace(/\\/g, '/')}"`).join('\n'));
if (process.env.SHOW) console.log(kept.filter(a => !a.endsWith('.rlib') && !a.endsWith('.o')).join('\n'));
const r = spawnSync(exe, ['@' + rsp], { encoding: 'utf8', maxBuffer: 1 << 28 });
console.log(r.stdout, r.stderr, r.error || '');
process.exit(r.status ?? 1);
