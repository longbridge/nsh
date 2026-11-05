// Machine translation of the documentation (see "Other languages" in the
// README). Only pages whose English changed are sent; a page that fails its
// checks keeps its previous translation, or falls back to the English.
//
//   TRANSLATE_API_KEY=… TRANSLATE_API_URL=https://…/v1 TRANSLATE_MODEL=… \
//     bun scripts/translate.js --source <dir with docs, elements, modules> --out <dir>
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chunks, keepAnchors, mask, reject, splitPage, unmask } from '../src/lib/translate.js';

const website = fileURLToPath(new URL('..', import.meta.url));
const flag = (name, fallback) => {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? fallback : process.argv[at + 1];
};
const source = flag('source', website);
const out = flag('out', website);
const only = flag('locale');
const limit = Number(flag('limit', Infinity));
const { TRANSLATE_API_KEY: key, TRANSLATE_API_URL: api, TRANSLATE_MODEL: model } = process.env;
if (!key || !api || !model) {
  console.error('[translate] set TRANSLATE_API_KEY, TRANSLATE_API_URL and TRANSLATE_MODEL');
  process.exit(1);
}

const SECTIONS = ['docs', 'elements', 'modules'];
const UI = {
  notice: 'This page was translated by machine and may contain errors.',
  fallback: 'This page has not been translated yet.',
  original: 'Read the English original',
  prev: 'Previous',
  next: 'Next',
  toc: 'On this page',
  Docs: 'Docs',
  Elements: 'Elements',
  'Start here': 'Start here',
  Platforms: 'Platforms',
  'Delivery and security': 'Delivery and security',
  'Building UI': 'Building UI',
  'Tools and internals': 'Tools and internals',
  Primitives: 'Primitives',
};
const usage = { input: 0, output: 0 };

async function complete(system, user) {
  for (let attempt = 0; ; attempt++) {
    const response = await fetch(`${api.replace(/\/$/, '')}/chat/completions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${key}` },
      body: JSON.stringify({ model, messages: [{ role: 'system', content: system }, { role: 'user', content: user }] }),
    });
    if (response.ok) {
      const data = await response.json();
      usage.input += data.usage?.prompt_tokens ?? 0;
      usage.output += data.usage?.completion_tokens ?? 0;
      return data.choices[0].message.content.trim();
    }
    if (attempt >= 3) throw new Error(`${response.status} ${await response.text()}`);
    await new Promise((done) => setTimeout(done, 2000 * 2 ** attempt));
  }
}

const system = (name) => `You translate the documentation of Native Shell, a native UI framework, from English into ${name}, for developers.
Output only the translation of the Markdown you are given, with nothing around it.
Keep every token like ⟦12⟧ exactly once; they stand for code and links. You may move a token within its sentence.
Keep the Markdown structure: the same headings at the same levels, lists, tables, blockquotes.
Keep in English: Native Shell, NSH, and names of APIs, components, platforms and tools.`;

async function translatePage(name, content) {
  const { front, body } = splitPage(content);
  const { text, kept } = mask(body);
  const parts = [];
  for (const piece of chunks(text)) {
    let answer = await complete(system(name), piece);
    let why = reject(piece, answer);
    if (why) {
      answer = await complete(system(name), piece);
      why = reject(piece, answer);
    }
    if (why) throw new Error(why);
    parts.push(answer);
  }
  const fields = JSON.parse(stripFence(await complete(`${system(name)}\nThe input is a JSON object: answer with the same keys and translated values, as JSON.`, JSON.stringify({ title: front.title, description: front.description }))));
  const lines = ['---', `title: ${JSON.stringify(fields.title)}`, `description: ${JSON.stringify(fields.description)}`];
  for (const field of ['order', 'group', 'demo']) if (front[field] !== undefined) lines.push(`${field}: ${front[field]}`);
  lines.push(`source_hash: ${hash(content)}`, '---', '');
  return `${lines.join('\n')}\n${unmask(keepAnchors(text, parts.join('\n')), kept)}\n`;
}

const stripFence = (text) => text.replace(/^```(?:json)?\s*|\s*```$/g, '');
const hash = (text) => createHash('sha256').update(text).digest('hex').slice(0, 16);

async function pool(items, size, run) {
  let next = 0;
  await Promise.all(Array.from({ length: size }, async () => {
    while (next < items.length) await run(items[next++]);
  }));
}

const { machine } = JSON.parse(readFileSync(join(website, 'locales.json'), 'utf8'));
let failed = 0;
let done = 0;
let budget = limit;
for (const { code, name } of machine) {
  if (only && only !== code) continue;
  const todo = [];
  for (const section of SECTIONS) {
    const dir = join(source, section);
    const files = readdirSync(dir).filter((file) => file.endsWith('.md'));
    for (const file of files) {
      const content = readFileSync(join(dir, file), 'utf8');
      const target = join(out, code, section, file);
      const current = existsSync(target) ? splitPage(readFileSync(target, 'utf8')).front.source_hash : undefined;
      if (current !== hash(content) && budget-- > 0) todo.push({ content, target, label: `${code}/${section}/${file}` });
    }
    // A page the English no longer has.
    const own = join(out, code, section);
    if (existsSync(own)) for (const file of readdirSync(own)) if (!files.includes(file)) rmSync(join(own, file));
  }
  await pool(todo, 4, async ({ content, target, label }) => {
    try {
      const page = await translatePage(name, content);
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, page);
      done++;
      console.log(`[translate] ${label}`);
    } catch (error) {
      failed++;
      console.error(`[translate] ${label}: ${error.message}`);
    }
  });
  const uiFile = join(out, code, 'ui.json');
  if (!existsSync(uiFile) || Object.keys(UI).some((k) => !(k in JSON.parse(readFileSync(uiFile, 'utf8'))))) {
    const strings = JSON.parse(stripFence(await complete(`${system(name)}\nThe input is a JSON object: answer with the same keys and translated values, as JSON.`, JSON.stringify(UI))));
    mkdirSync(dirname(uiFile), { recursive: true });
    writeFileSync(uiFile, `${JSON.stringify(strings, null, 2)}\n`);
  }
}
console.log(`[translate] ${done} translated, ${failed} failed, tokens in ${usage.input} out ${usage.output}`);
// Failures keep the fallback, and the next release retries them; all failing is a broken setup.
if (failed && !done) process.exit(1);
