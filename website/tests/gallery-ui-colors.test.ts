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

// The demos' switches and toggles take the shared looks (`switchLook`,
// `toggleLook`): parts or an on state styled in place would be another copy
// of them, as the inline ones were (a switch's knob ran 2 points past the
// track with a pointer; light, a raised or sunk on state was all but the
// color of the surface under it).

/** Each opening tag of the element `name` in a source (`<Toggle ...>`, not
 * `<ToggleGroup ...>`): up to the `>` outside braces, past `//` comments. */
function openingTags(source: string, name: string): string[] {
  const tags: string[] = [];
  for (const match of source.matchAll(new RegExp(`<${name}\\b`, 'g'))) {
    let depth = 0;
    let i = match.index + match[0].length;
    for (; i < source.length; i++) {
      const c = source[i];
      if (c === '{') depth++;
      else if (c === '}') depth--;
      else if (depth === 0 && c === '/' && source[i + 1] === '/') i = source.indexOf('\n', i);
      else if (depth === 0 && c === '>') break;
    }
    tags.push(source.slice(match.index, i + 1));
  }
  return tags;
}

/** What is wrong with a switch's tag: styled parts, or no shared look. */
function switchViolations(tag: string): string[] {
  const found = [...tag.matchAll(/\b(track|thumb)(Class|Style)=/g)].map((m) => `sets ${m[1]}${m[2]}`);
  if (!/\{\.\.\.switchLook\(/.test(tag)) found.push('does not spread switchLook(..)');
  return found;
}

test('the switch guard finds styled parts and a missing look', () => {
  const styled = '<Switch checked={on()} trackClass="w-9" thumbStyle={{ x: 1 }}>\n  <Text>a</Text>\n</Switch>';
  expect(openingTags(styled, 'Switch')).toHaveLength(1);
  expect(switchViolations(openingTags(styled, 'Switch')[0])).toEqual(['sets trackClass', 'sets thumbStyle', 'does not spread switchLook(..)']);
  const shared = '<Switch\n  onChange={({ checked }) => on.set(checked)}\n  {...switchLook(() => on())}\n  // a note > here\n  hitSlop={0}\n>';
  expect(openingTags(shared, 'Switch')).toEqual([shared]);
  expect(switchViolations(shared)).toEqual([]);
});

test('the demos\' switches spread the shared look', () => {
  const report: string[] = [];
  for (const file of files('website/demos')) {
    for (const tag of openingTags(readFileSync(join(root, file), 'utf8'), 'Switch')) {
      for (const what of switchViolations(tag)) report.push(`${file}: ${what}: ${tag.split('\n')[0]}`);
    }
  }
  expect(report).toEqual([]);
});

/** What is wrong with a toggle's tag: no shared look. */
function toggleViolations(tag: string): string[] {
  return /\{\.\.\.toggleLook\(/.test(tag) ? [] : ['does not spread toggleLook(..)'];
}

test('the toggle guard finds a toggle without the shared look', () => {
  const styled = '<Toggle pressed={on()} pressedStyle={ui().raised} style={{ width: 28 }}>\n  <Text>B</Text>\n</Toggle>';
  expect(openingTags(styled, 'Toggle')).toHaveLength(1);
  expect(toggleViolations(openingTags(styled, 'Toggle')[0])).toEqual(['does not spread toggleLook(..)']);
  const shared = '<Toggle\n  pressed={on()}\n  onChange={({ pressed }) => on.set(pressed)}\n  // a note > here\n  {...toggleLook({ variant: "outline" })}\n>';
  expect(openingTags(shared, 'Toggle')).toEqual([shared]);
  expect(toggleViolations(shared)).toEqual([]);
  expect(openingTags('<ToggleGroup style={track}>', 'Toggle')).toEqual([]);
});

test('the demos\' toggles spread the shared look', () => {
  const report: string[] = [];
  for (const file of files('website/demos')) {
    for (const tag of openingTags(readFileSync(join(root, file), 'utf8'), 'Toggle')) {
      for (const what of toggleViolations(tag)) report.push(`${file}: ${what}: ${tag.split('\n')[0]}`);
    }
  }
  expect(report).toEqual([]);
});
