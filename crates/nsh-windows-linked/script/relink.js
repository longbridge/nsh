// Relinks the full runner from its saved objects with only the given parts,
// as `nsh package` would: node relink.js <linkcmd.txt> <out.exe> [editor] [plot]
const fs = require('fs'), { spawnSync } = require('child_process');
const [cmdFile, out, ...parts] = process.argv.slice(2);
const line = fs.readFileSync(cmdFile, 'utf8').trim();
const args = [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map(m => m[1].replace(/\\\\/g, '\\').replace(/\\"/g, '"'));
const exe = args.shift();
const all = ['editor', 'plot'];
const kept = args.filter(a => {
  if (a.startsWith('/NATVIS') || a.startsWith('/OUT:') || a.includes('Visual Studio')) return false;
  for (const p of all) if (!parts.includes(p) && /libnsh_windows_(\w+)\.r?lib$/.exec(a)?.[1] === p) return false;
  return true;
});
for (const p of all) if (!parts.includes(p)) kept.push(`/ALTERNATENAME:nsh_part_${p}=nsh_part_none`);
kept.push(`/OUT:${out}`);
if (process.env.EXTRA) kept.push(...process.env.EXTRA.split(' '));
if (process.env.SHOW) console.log(kept.filter(a => !a.endsWith('.rlib')).join('\n'));
// The command line is too long for Windows: pass it in a response file.
const rsp = out + '.rsp';
fs.writeFileSync(rsp, kept.map(a => `"${a}"`).join('\n'));
// LINKER=rust-lld.exe links without Visual Studio (LIB names the folder of
// the system import libraries).
const lld = process.env.LINKER;
const r = lld
  ? spawnSync(lld, ['-flavor', 'link', '@' + rsp], { encoding: 'utf8' })
  : spawnSync(exe, ['@' + rsp], { encoding: 'utf8' });
console.log(r.stdout, r.stderr, r.error || '');
process.exit(r.status ?? 1);
