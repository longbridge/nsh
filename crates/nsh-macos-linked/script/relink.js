// Relinks the full macOS runner from rustc's saved link command with only
// the given parts, as `nsh package` would:
//   node relink.js <linkcmd.txt> <out> [editor] [plot]
// linkcmd.txt is the `cc` line of `--print link-args`. LINKER=driver (the
// default) reruns it through `cc`; LINKER=lld runs rust-lld's Mach-O
// linker on it with SDK=<a directory of .tbd stubs> and no Xcode.
const fs = require('fs'), path = require('path'), { spawnSync } = require('child_process');
const [cmdFile, out, ...parts] = process.argv.slice(2);
const line = fs.readFileSync(cmdFile, 'utf8').trim();
let args = [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map(m => m[1].replace(/\\"/g, '"').replace(/\\\\/g, '\\'));
const cc = args.findIndex(a => a === 'cc' || a.endsWith('/cc'));
args = args.slice(cc + 1);
const all = ['editor', 'plot'];
const kept = [];
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === '-o') { i++; continue; }
  const m = /libnsh_macos_([a-z]+)-[0-9a-f]+\.(rlib|a)$/.exec(a);
  if (m && all.includes(m[1]) && !parts.includes(m[1])) continue;
  kept.push(a);
}
const alias = all.filter(p => !parts.includes(p)).map(p => ['_nsh_part_none', `_nsh_part_${p}`]);
let r;
if ((process.env.LINKER || 'driver') === 'driver') {
  for (const [from, to] of alias) kept.push(`-Wl,-alias,${from},${to}`);
  kept.push('-o', out);
  r = spawnSync('cc', kept, { encoding: 'utf8', maxBuffer: 1 << 28 });
} else {
  // The `cc` driver's flags as ld64 flags.
  const ld = ['-arch', 'arm64', '-platform_version', 'macos', process.env.MIN || '11.0', process.env.SDKVER || '15.0',
    '-syslibroot', process.env.SDK, '-o', out, '-lSystem'];
  for (let i = 0; i < kept.length; i++) {
    const a = kept[i];
    if (a === '-arch') { i++; continue; }
    if (a.startsWith('-mmacosx-version-min') || a === '-nodefaultlibs' || a.startsWith('-fuse-ld')) continue;
    if (a.startsWith('-Wl,')) { ld.push(...a.slice(4).split(',')); continue; }
    if (a === '-framework') { ld.push(a, kept[++i]); continue; }
    ld.push(a);
  }
  for (const [from, to] of alias) ld.push('-alias', from, to);
  if (process.env.EXTRA) ld.push(...process.env.EXTRA.split(' '));
  const lld = process.env.RUST_LLD;
  r = spawnSync(lld, ['-flavor', 'darwin', ...ld], { encoding: 'utf8', maxBuffer: 1 << 28, env: { PATH: '/usr/bin:/bin' } });
}
if (process.env.SHOW) console.log(kept.filter(a => !/\.(rlib|o|a)$/.test(a)).join('\n'));
process.stdout.write(r.stdout || ''); process.stderr.write(r.stderr || '');
if (r.error) console.error(r.error);
process.exit(r.status ?? 1);
