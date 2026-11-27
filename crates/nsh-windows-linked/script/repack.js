// Repacks the objects objsize.js extracted into one .lib per rlib (no rustc
// metadata), and writes a link command that uses them instead of the rlibs:
// node repack.js <linkcmd.txt> <objdir> <libdir> <out-linkcmd.txt>
const fs = require('fs'), path = require('path'), { spawnSync } = require('child_process');
const [cmdFile, objDir, libDir, outCmd] = process.argv.slice(2);
const lld = process.env.LINKER;
fs.mkdirSync(libDir, { recursive: true });
const objs = fs.readdirSync(objDir);
let line = fs.readFileSync(cmdFile, 'utf8');
for (const m of [...line.matchAll(/"([^"]*\.rlib)"/g)]) {
  const lib = m[1].replace(/\\\\/g, '\\');
  const stem = path.win32.basename(lib, '.rlib');
  const mine = objs.filter(o => o.startsWith(stem + '-')).map(o => path.join(objDir, o));
  const out = path.join(libDir, stem + '.lib');
  const rsp = out + '.rsp';
  fs.writeFileSync(rsp, mine.map(o => `"${o}"`).join('\n'));
  const r = spawnSync(lld, ['-flavor', 'link', '/lib', `/out:${out}`, '@' + rsp], { encoding: 'utf8' });
  if (r.status) { console.log(stem, r.stdout, r.stderr); process.exit(1); }
  fs.unlinkSync(rsp);
  line = line.replace(m[0], `"${out.replace(/\\/g, '\\\\')}"`);
}
fs.writeFileSync(outCmd, line);
