# Writing user docs

These rules cover the website docs (`website/docs/`, `website/zh-CN/docs/`) and the home page copy
(`website/src/home/*.json`). AI tools read the docs as well, but people come first. If a person
can't follow a page, it has failed, however complete it is.

## The test

Read each paragraph aloud. If you would never say it that way to a colleague, rewrite it.

## Order of a page

1. **What it is**: one or two plain sentences.
2. **Why you would use it**, or what problem it solves.
3. **The smallest working example.**
4. **The details**: options, limits, edge cases, internals.

Put the common case first and rare cases last.

## Sentences

- Keep sentences short, with one idea each. Split any sentence that has more than one clause joined by
  `;`, `,` or `which`.
- Use the subject–verb–object order: "NSH checks every `fetch` against the allowed hosts", not "every
  `fetch` goes through, under what a host grants, the same checks".
- Use concrete nouns: name the API, the file and the platform. Avoid "the thing that", "the rest of
  that app", "as X is".
- Prefer the active voice and present tense. Address the reader as "you".
- Don't hedge or pad. Remove "It's important to note", "seamlessly", "robust", "powerful",
  "leverage", "in order to", "simply", "effectively", "comprehensive" and "first-class".
- Don't repeat a point in other words. Say it once, where it belongs.
- Don't use marketing adjectives without a number or an example behind them.

## Technical depth

Keep it. Short does not mean shallow. Keep the exact limits, defaults, error names and
platform differences, and present them as tables or lists instead of long sentences.

## Chinese pages

Write zh-CN pages directly in natural Chinese, not as a sentence-by-sentence translation of the
English page. The content must match; the wording does not have to. Avoid translation patterns
such as long "的" chains, "被" passives and literal English clause order. Keep API names, code and
units as they are.

## Before you submit

- Read the first paragraph on its own. It must tell a newcomer what the page is about.
- Check every number and claim against the code or a benchmark.
- AI may draft a page, but a person must edit it before it ships.
