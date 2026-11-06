import { expect, test } from 'bun:test';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

// The shared looks and the demos' adapters take every color from the theme
// (`colors`, `tints`, `finish`, `token(..)`), so both apps follow its light
// and dark appearances and a theme change reaches every demo. A color
// written out, or a token's value spliced into a class string (read once,
// when the class is built), would not.

const root = join(import.meta.dir, '../..');

/** What the guard reads: files, or directories read with what they hold. */
const scanned = [
  'examples/nsh-gallery-ui/demo.ts',
  'examples/nsh-gallery-ui/palette.ts',
  'examples/nsh-gallery-ui/look',
];

/** Files the guard skips, each with why its colors are content rather than
 * the interface's look (a canvas's drawing, a shader, a color picker's
 * swatches). Empty while only the shared layer is scanned. */
const allowed: Record<string, string> = {};

/** Color literals and spliced token values, by what each is. */
const rules: [string, RegExp][] = [
  ['a hex color', /#[0-9a-fA-F]{3,8}\b/],
  ['an rgb() / rgba() color', /\brgba?\(/],
  ['an hsl() / hsla() color', /\bhsla?\(/],
  ['a color-mix() (use the theme\'s alpha)', /\bcolor-mix\(/],
  ['an arbitrary color class', /\b(?:text|bg|border|fill|stroke|ring|outline|from|via|to)-\[#/],
  ['a value spliced into a class', /-\[\$\{/],
];

function files(path: string): string[] {
  const full = join(root, path);
  try {
    return readdirSync(full, { recursive: true, encoding: 'utf8' })
      .filter((name) => /\.(?:ts|tsx)$/.test(name))
      .map((name) => `${path}/${name}`);
  } catch {
    return [path];
  }
}

/** The source without its comments, which may cite an issue (`#1026`) or
 * name a color in prose. A `//` after a colon is a URL's, not a comment. */
function code(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1');
}

function violations(source: string): string[] {
  const found: string[] = [];
  code(source).split('\n').forEach((line, i) => {
    for (const [what, pattern] of rules) {
      if (pattern.test(line)) found.push(`${i + 1}: ${what}: ${line.trim()}`);
    }
  });
  return found;
}

test('the guard finds color literals and spliced values, not comments or tokens', () => {
  expect(violations('const a = "#c6ef6a";')).toHaveLength(1);
  expect(violations('borderColor: "rgba(0,0,0,0)",')).toHaveLength(1);
  expect(violations('color: "hsl(80 70% 60%)",')).toHaveLength(1);
  expect(violations('class: "text-[#fff] p-2",')).toHaveLength(2);
  expect(violations('class: `text-[${ui().text}]`,')).toHaveLength(1);
  expect(violations('// see #1026: rgba(0,0,0,0.4) once\nconst c = colors.text;')).toHaveLength(0);
  expect(violations('/* #ffffff */ const c = token("accent");')).toHaveLength(0);
  expect(violations('const url = "https://example.com/#top";')).toHaveLength(0);
});

test('the shared looks and the demo adapters take their colors from the theme', () => {
  const report: string[] = [];
  for (const file of scanned.flatMap(files)) {
    if (Object.hasOwn(allowed, file)) continue;
    for (const line of violations(readFileSync(join(root, file), 'utf8'))) report.push(`${file}:${line}`);
  }
  expect(report).toEqual([]);
});

test('every allowed file still exists and says why', () => {
  for (const [file, reason] of Object.entries(allowed)) {
    expect(readFileSync(join(root, file), 'utf8').length).toBeGreaterThan(0);
    expect(reason.length).toBeGreaterThan(0);
  }
});
