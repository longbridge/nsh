// Writes .tbd stubs that list only the symbols a linked runner imports
// from each system library, in an SDK-shaped directory rust-lld can use as
// -syslibroot: node stubs.js <exe> <outdir>
// Uses the Rust toolchain's llvm-objdump / llvm-nm (LLVM_BIN), not Xcode's.
const fs = require('fs'), path = require('path'), { execFileSync } = require('child_process');
const [exe, outDir] = process.argv.slice(2);
const bin = process.env.LLVM_BIN;
const run = (tool, args) => execFileSync(path.join(bin, tool), args, { encoding: 'utf8', maxBuffer: 1 << 28 });
// Install names, in load-command order (the ordinals nm's "from" names).
const dylibs = run('llvm-objdump', ['--macho', '--dylibs-used', exe]).split('\n').slice(1)
  .map(l => l.trim().split(' (')[0]).filter(Boolean);
const short = n => {
  const fw = /\/([^/]+)\.framework\//.exec(n);
  if (fw) return fw[1];
  return path.basename(n).replace(/\.dylib$/, '').replace(/\.[A-Z0-9]+$/, '').replace(/\.\d+(\.\d+)*$/, '');
};
const syms = new Map(dylibs.map(d => [short(d), new Set()]));
for (const l of run('llvm-nm', ['-m', '-u', exe]).split('\n')) {
  const m = /\(undefined\)(?: weak)? external (\S+) \(from ([^)]+)\)/.exec(l);
  if (!m) continue;
  if (!syms.has(m[2])) { console.error('unknown library', m[2], m[1]); process.exit(1); }
  syms.get(m[2]).add(m[1]);
}
const REEXPORTS = { Carbon: ["CoreServices"] };
let total = 0;
for (const d of dylibs) {
  const s = [...syms.get(short(d))].sort();
  total += s.length;
  const fw = /^(.*\/([^/]+)\.framework)\//.exec(d);
  // Re-exports the system has and the link names only by their parent
  // (Carbon re-exports CoreServices).
  const re = (REEXPORTS[short(d)] || []).map(c => dylibs.find(x => short(x) === c)).filter(Boolean);
  const file = fw ? path.join(outDir, fw[1], fw[2] + '.tbd')
    : path.join(outDir, path.dirname(d), path.basename(d).replace(/(\.[A-Z])?(\.\d+)*\.dylib$/, '') + '.tbd');
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const list = s.map(x => `'${x}'`).join(', ');
  fs.writeFileSync(file, `--- !tapi-tbd\ntbd-version: 4\ntargets: [ arm64-macos ]\ninstall-name: '${d}'\n` +
    (re.length ? `reexported-libraries:\n  - targets: [ arm64-macos ]\n    libraries: [ ${re.map(x => `'${x}'`).join(", ")} ]\n` : "") +
    (s.length ? `exports:\n  - targets: [ arm64-macos ]\n    symbols: [ ${list} ]\n` : '') + '...\n');
  // The install name too: where a re-exported library is looked up.
  if (d.startsWith("/System/")) { const p = path.join(outDir, d + ".tbd"); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.copyFileSync(file, p); }
  console.log(`${s.length}\t${d}\t${path.relative(outDir, file)}`);
}
console.log(`${total} symbols, ${dylibs.length} libraries`);
// -lc and -lm name libSystem, as in the SDK.
for (const l of ['libc', 'libm']) fs.copyFileSync(path.join(outDir, 'usr/lib/libSystem.tbd'), path.join(outDir, `usr/lib/${l}.tbd`));
