// Sums the members of the rlibs on the link line by kind, and extracts the
// object files into <outdir> (what a shipped part would hold):
// node objsize.js <linkcmd.txt> <outdir>
const fs = require('fs'), path = require('path');
const [cmdFile, outDir] = process.argv.slice(2);
const line = fs.readFileSync(cmdFile, 'utf8');
const libs = [...line.matchAll(/"([^"]*\.rlib)"/g)].map(m => m[1].replace(/\\\\/g, '\\'));
fs.mkdirSync(outDir, { recursive: true });
const kinds = {};
let names = null;
for (const lib of libs) {
  const buf = fs.readFileSync(lib);
  let off = 8, i = 0;
  while (off + 60 <= buf.length) {
    const hdr = buf.toString('latin1', off, off + 60);
    let name = hdr.slice(0, 16).trim();
    const size = parseInt(hdr.slice(48, 58).trim(), 10);
    const data = buf.subarray(off + 60, off + 60 + size);
    if (name === '//') names = data.toString('latin1');
    else if (name.startsWith('/') && /^\/\d+/.test(name) && names) {
      const at = parseInt(name.slice(1), 10);
      name = names.slice(at).split(/\0|\/\n/)[0];
    }
    name = name.replace(/\/$/, '');
    const kind = name.endsWith('.o') ? 'obj' : name === 'lib.rmeta' ? 'rmeta' : name === '' || name.startsWith('/') ? 'symtab' : 'other';
    kinds[kind] = (kinds[kind] || 0) + size;
    if (kind === 'obj') fs.writeFileSync(path.join(outDir, `${path.basename(lib, '.rlib')}-${i++}-${path.win32.basename(name)}`), data);
    off += 60 + size + (size & 1);
  }
  names = null;
}
for (const [k, v] of Object.entries(kinds)) console.log(k, (v / 1048576).toFixed(1), 'MB');
